mod config;
mod filters;
mod value_multiset;


use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::convert::Infallible;
use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::LazyLock;

use askama::Template;
use ciborium::cbor;
use form_urlencoded;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, Response};
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use percent_encoding;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio_postgres::types::ToSql;
use toml;
use tracing::{error, instrument, warn};
use tracing_subscriber;

use crate::config::{CONFIG, Config};
use crate::value_multiset::ValueMultiset;


static STATIC_FILE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(concat!(
    "^",
    "[A-Za-z0-9_-]+",
    "(?:",
        "[.]",
        "[A-Za-z0-9_-]+",
    ")*",
    "$",
)).expect("failed to compile static file regex"));


#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct BimPart {
    pub id: i64,
    pub company: String,
    pub veh_number: String,
    pub type_code: String,
    pub veh_class: String,
    pub in_service_since: Option<String>,
    pub out_of_service_since: Option<String>,
    pub manufacturer: Option<String>,
    pub depot: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct CouplingPart {
    pub id: i64,
    pub vehicles: Vec<CouplingVehiclePart>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct CouplingVehiclePart {
    pub id: i64,
    pub veh_number: String,
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    pub companies: BTreeSet<String>,
    pub vehicles: Vec<BimPart>,
    pub base_path: String,
    pub page: i64,
    pub company: String,
}

#[derive(Template)]
#[template(path = "add_edit.html")]
struct AddEditTemplate {
    pub base_path: String,
    pub edit_id: Option<i64>,
    pub company: Option<String>,
    pub veh_number: Option<String>,
    pub type_code: Option<String>,
    pub veh_class: Option<String>,
    pub power_sources: BTreeSet<String>,
    pub in_service_since: Option<String>,
    pub out_of_service_since: Option<String>,
    pub manufacturer: Option<String>,
    pub depot: Option<String>,
    pub other_data: Option<String>,
    pub allowed_veh_classes: BTreeSet<String>,
    pub allowed_power_sources: BTreeSet<String>,
}

#[derive(Template)]
#[template(path = "coupling_list.html")]
struct CouplingListTemplate {
    pub base_path: String,
    pub couplings: Vec<CouplingPart>,
}

#[derive(Template)]
#[template(path = "coupling_add_edit.html")]
struct CouplingAddEditTemplate {
    pub base_path: String,
    pub edit_id: Option<i64>,
    pub company_to_vehicles: BTreeMap<String, BTreeSet<String>>,
    pub company: Option<String>,
    pub vehicles: Vec<String>,
}
impl CouplingAddEditTemplate {
    pub fn company_to_vehicles_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.company_to_vehicles)
            .expect("failed to serialize company-to-uncoupled-vehicles to JSON")
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum ExportFormat {
    Json,
    Cbor,
}


fn path_to_parts(path: &str, strip_first_empty: bool) -> Option<Vec<Cow<str>>> {
    let mut ret = Vec::new();
    let mut first_round = true;
    for piece in path.split('/') {
        let part = percent_encoding::percent_decode_str(piece)
            .decode_utf8().ok()?;
        if !(strip_first_empty && first_round && part.len() == 0) {
            ret.push(part);
        }
        first_round = false;
    }
    Some(ret)
}


fn strip_path_prefix<'h, 'n, H: AsRef<str>, N: AsRef<str>>(haystack: &'h [H], needle: &'n [N]) -> Option<&'h [H]> {
    if needle.len() > haystack.len() {
        return None;
    }
    for (h, n) in haystack.iter().zip(needle.iter()) {
        if h.as_ref() != n.as_ref() {
            return None;
        }
    }
    Some(&haystack[needle.len()..])
}


fn return_500() -> Response<Full<Bytes>> {
    Response::builder()
        .status(500)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from("500 Internal Server Error")))
        .expect("failed to construct HTTP 500 response")
}
fn return_400(reason: &str) -> Response<Full<Bytes>> {
    let body_string = format!("400 Bad Request: {}", reason);
    Response::builder()
        .status(400)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from(body_string)))
        .unwrap_or_else(|_| return_500())
}
fn return_404() -> Response<Full<Bytes>> {
    Response::builder()
        .status(400)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from("404 Not Found")))
        .unwrap_or_else(|_| return_500())
}
fn return_405(method: &Method, allowed_methods: &[Method]) -> Response<Full<Bytes>> {
    let allowed_methods: Vec<&str> = allowed_methods.iter().map(|m| m.as_str()).collect();
    let allowed_methods_string = allowed_methods.join(", ");
    let body_text = format!("unsupported method {}; allowed: {}", method, allowed_methods_string);
    Response::builder()
        .status(405)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Allow", &allowed_methods_string)
        .body(Full::new(Bytes::from(body_text)))
        .unwrap_or_else(|_| return_500())
}


async fn db_connect() -> Option<tokio_postgres::Client> {
    let db_config = &CONFIG
        .get().expect("CONFIG not set?!")
        .db;
    let connect_res = tokio_postgres::Config::new()
        .host(&db_config.hostname)
        .user(&db_config.username)
        .password(&db_config.password)
        .dbname(&db_config.db_name)
        .port(db_config.port)
        .connect(tokio_postgres::NoTls).await;
    let (client, connection) = match connect_res {
        Ok(cc) => cc,
        Err(e) => {
            error!("failed to connect to database: {}", e);
            return None;
        },
    };
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            error!("Postgres connection error: {}", e);
        }
    });
    Some(client)
}

fn cow_replace<'t, 'o, 'n>(text: Cow<'t, str>, old: &'o str, new: &'n str) -> Cow<'t, str> {
    if text.contains(old) {
        Cow::Owned(text.replace(old, new))
    } else {
        text
    }
}

fn get_query_pairs(query: Option<&str>) -> Option<Vec<(String, Option<String>)>> {
    if let Some(q) = query {
        let mut parts = Vec::new();
        for piece in q.split('&') {
            match piece.split_once('=') {
                Some((k, v)) => {
                    let k_plus = cow_replace(Cow::Borrowed(k), "+", " ");
                    let v_plus = cow_replace(Cow::Borrowed(v), "+", " ");
                    let k_parsed = percent_encoding::percent_decode_str(k_plus.as_ref())
                        .decode_utf8().ok()?
                        .into_owned();
                    let v_parsed = percent_encoding::percent_decode_str(v_plus.as_ref())
                        .decode_utf8().ok()?
                        .into_owned();
                    parts.push((k_parsed, Some(v_parsed)));
                },
                None => {
                    // key without value
                    let k_plus = cow_replace(Cow::Borrowed(piece), "+", " ");
                    let k_parsed = percent_encoding::percent_decode_str(k_plus.as_ref())
                        .decode_utf8().ok()?
                        .into_owned();
                    parts.push((k_parsed, None));
                },
            }
        }
        Some(parts)
    } else {
        Some(Vec::with_capacity(0))
    }
}


#[instrument(skip_all)]
async fn handle_index(_remote_addr: SocketAddr, request: Request<Incoming>) -> Response<Full<Bytes>> {
    if request.method() != Method::GET {
        return return_405(request.method(), &[Method::GET]);
    }

    let query_pairs = match get_query_pairs(request.uri().query()) {
        Some(qp) => qp,
        None => return return_400("invalid UTF-8 in query"),
    };

    let db_conn = match db_connect().await {
        Some(dbc) => dbc,
        None => return return_500(),
    };

    // obtain companies
    let company_rows_res = db_conn.query(
        "SELECT DISTINCT company FROM bimdb.bims",
        &[],
    ).await;
    let company_rows = match company_rows_res {
        Ok(cr) => cr,
        Err(e) => {
            error!("failed to obtain companies: {}", e);
            return return_500();
        },
    };
    let mut companies = BTreeSet::new();
    for row in company_rows {
        let company: String = row.get(0);
        companies.insert(company);
    }

    // obtain vehicles
    let per_page = CONFIG
        .get().expect("CONFIG not set?!")
        .vehicles_per_page;
    let page_str = query_pairs.iter()
        .filter(|(k, _v)| k == "page")
        .map(|(_k, v)| v.as_ref().map(|v2| v2.as_str()))
        .flatten()
        .last()
        .unwrap_or("0");
    let company_str = query_pairs.iter()
        .filter(|(k, _v)| k == "company")
        .map(|(_k, v)| v.as_ref().map(|v2| v2.as_str().trim()))
        .flatten()
        .last()
        .unwrap_or("");
    let page: i64 = match page_str.parse() {
        Ok(pn) => if pn < 0 {
            return return_400(&format!("'page' must be >= 0"));
        } else {
            pn
        },
        Err(_) => return return_400("invalid 'page'"),
    };
    let page_offset = page * per_page;
    let mut query_params: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(3);
    query_params.push(&per_page);
    query_params.push(&page_offset);
    let query = format!(
        "
            SELECT
                id, company, veh_number, type_code,
                veh_class, in_service_since, out_of_service_since, manufacturer,
                depot
            FROM
                bimdb.bims
            {}
            ORDER BY
                company, veh_number, id
            LIMIT $1 OFFSET $2
        ",
        if company_str.len() > 0 { "WHERE company = $3" } else { "" },
    );
    if company_str.len() > 0 {
        query_params.push(&company_str);
    }
    let vehicle_rows = match db_conn.query(&query, &query_params).await {
        Ok(vr) => vr,
        Err(e) => {
            error!("failed to obtain vehicle rows: {}", e);
            return return_500();
        },
    };

    let mut vehicles = Vec::new();
    for row in vehicle_rows {
        let id: i64 = row.get(0);
        let company: String = row.get(1);
        let veh_number: String = row.get(2);
        let type_code: String = row.get(3);
        let veh_class: String = row.get(4);
        let in_service_since: Option<String> = row.get(5);
        let out_of_service_since: Option<String> = row.get(6);
        let manufacturer: Option<String> = row.get(7);
        let depot: Option<String> = row.get(8);
        vehicles.push(BimPart {
            id,
            company,
            veh_number,
            type_code,
            veh_class,
            in_service_since,
            out_of_service_since,
            manufacturer,
            depot,
        })
    }

    let config = CONFIG.get().expect("CONFIG not set?!");
    let template = IndexTemplate {
        companies,
        vehicles,
        base_path: config.http.base_path.clone(),
        page,
        company: company_str.to_owned(),
    };
    let template_text = template.render()
        .expect("failed to render template");
    Response::builder()
        .status(200)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Full::new(Bytes::from(template_text)))
        .unwrap_or_else(|_| return_500())
}

#[instrument(skip_all)]
async fn handle_export(_remote_addr: SocketAddr, request: Request<Incoming>, format: ExportFormat) -> Response<Full<Bytes>> {
    if request.method() != Method::GET {
        return return_405(request.method(), &[Method::GET]);
    }

    let query_pairs = match get_query_pairs(request.uri().query()) {
        Some(qp) => qp,
        None => return return_400("invalid UTF-8 in query"),
    };

    let company_opt = query_pairs.iter()
        .filter(|(k, _v)| k == "company")
        .map(|(_k, v)| v)
        .flatten()
        .last();
    let company = match company_opt {
        Some(c) => c,
        None => return return_400("required parameter 'company' missing"),
    };

    let db_conn = match db_connect().await {
        Some(dbc) => dbc,
        None => return return_500(),
    };

    // obtain fixed couplings
    let mut bim_id_to_coupling: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let coupling_rows_res = db_conn.query(
        "
            SELECT
                b.id, coupled.veh_number
            FROM
                bimdb.bims b
                INNER JOIN bimdb.coupling_bims bim2cpl
                    ON bim2cpl.bim_id = b.id
                INNER JOIN bimdb.coupling_bims cpl2bim
                    ON cpl2bim.coupling_id = bim2cpl.coupling_id
                INNER JOIN bimdb.bims coupled
                    ON coupled.id = cpl2bim.bim_id
            WHERE
                b.company = $1
            ORDER BY
                b.id, cpl2bim.position
        ",
        &[&company],
    ).await;
    let coupling_rows = match coupling_rows_res {
        Ok(cr) => cr,
        Err(e) => {
            error!("failed to obtain coupling rows for company {:?}: {}", e, company);
            return return_500();
        },
    };
    for coupling_row in coupling_rows {
        let bim_id: i64 = coupling_row.get(0);
        let coupled_number: String = coupling_row.get(1);
        bim_id_to_coupling
            .entry(bim_id)
            .or_insert_with(|| Vec::new())
            .push(coupled_number);
    }

    // obtain power sources
    let mut bim_id_to_power_sources: BTreeMap<i64, BTreeSet<String>> = BTreeMap::new();
    let power_source_rows_res = db_conn.query(
        "
            SELECT
                b.id, ps.power_source
            FROM
                bimdb.bims b
                INNER JOIN bimdb.power_sources ps
                    ON ps.bim_id = b.id
            WHERE
                b.company = $1
            ORDER BY
                b.id, ps.power_source
        ",
        &[&company],
    ).await;
    let power_source_rows = match power_source_rows_res {
        Ok(psr) => psr,
        Err(e) => {
            error!("failed to obtain power source rows for company {:?}: {}", e, company);
            return return_500();
        },
    };
    for power_source_row in power_source_rows {
        let bim_id: i64 = power_source_row.get(0);
        let power_source: String = power_source_row.get(1);
        bim_id_to_power_sources
            .entry(bim_id)
            .or_insert_with(|| BTreeSet::new())
            .insert(power_source);
    }

    // obtain vehicles
    let vehicle_rows_res = db_conn.query(
        "
        SELECT
            b.id, b.veh_number, b.type_code, b.veh_class,
            b.in_service_since, b.out_of_service_since, b.manufacturer, b.depot
            b.other_data
        FROM
            bimdb.bims b
        WHERE
            b.company = $1
        ORDER BY
            b.veh_number, b.id
        ",
        &[&company],
    ).await;
    let vehicle_rows = match vehicle_rows_res {
        Ok(vr) => vr,
        Err(e) => {
            error!("failed to obtain vehicle rows: {}", e);
            return return_500();
        },
    };

    let (data, content_type) = match format {
        ExportFormat::Json => {
            let mut vehicles = Vec::new();
            let empty_coupling = Vec::with_capacity(0);
            let no_power_sources = BTreeSet::new();
            for row in vehicle_rows {
                let bim_id: i64 = row.get(0);
                let veh_number: String = row.get(1);
                let type_code: String = row.get(2);
                let veh_class: String = row.get(3);
                let in_service_since: Option<String> = row.get(4);
                let out_of_service_since: Option<String> = row.get(5);
                let manufacturer: Option<String> = row.get(6);
                let depot: Option<String> = row.get(7);
                let other_data: serde_json::Value = row.get(8);

                let fixed_coupling = bim_id_to_coupling.get(&bim_id)
                    .unwrap_or(&empty_coupling);
                let power_sources = bim_id_to_power_sources.get(&bim_id)
                    .unwrap_or(&no_power_sources);

                vehicles.push(serde_json::json!({
                    "number": veh_number,
                    "vehicle_class": veh_class,
                    "type_code": type_code,
                    "in_service_since": in_service_since,
                    "out_of_service_since": out_of_service_since,
                    "manufacturer": manufacturer,
                    "depot": depot,
                    "other_data": other_data,
                    "fixed_coupling": fixed_coupling,
                    "power_sources": power_sources,
                }));
            }
            let json_data = match serde_json::to_string_pretty(&vehicles) {
                Ok(jt) => jt.into_bytes(),
                Err(e) => {
                    error!("failed to serialize vehicles to JSON: {}", e);
                    return return_500();
                },
            };
            (json_data, "application/json")
        },
        ExportFormat::Cbor => {
            let mut vehicles = Vec::new();
            let empty_coupling = Vec::with_capacity(0);
            let no_power_sources = BTreeSet::new();
            for row in vehicle_rows {
                let bim_id: i64 = row.get(0);
                let veh_number: String = row.get(1);
                let type_code: String = row.get(2);
                let veh_class: String = row.get(3);
                let in_service_since: Option<String> = row.get(4);
                let out_of_service_since: Option<String> = row.get(5);
                let manufacturer: Option<String> = row.get(6);
                let depot: Option<String> = row.get(7);
                let other_data: serde_json::Value = row.get(8);

                let fixed_coupling = bim_id_to_coupling.get(&bim_id)
                    .unwrap_or(&empty_coupling);
                let power_sources = bim_id_to_power_sources.get(&bim_id)
                    .unwrap_or(&no_power_sources);

                let cbor_value_res = cbor!({
                    "number" => veh_number,
                    "vehicle_class" => veh_class,
                    "type_code" => type_code,
                    "in_service_since" => in_service_since,
                    "out_of_service_since" => out_of_service_since,
                    "manufacturer" => manufacturer,
                    "depot" => depot,
                    "other_data" => other_data,
                    "fixed_coupling" => fixed_coupling,
                    "power_sources" => power_sources,
                });
                let cbor_value = match cbor_value_res {
                    Ok(v) => v,
                    Err(e) => {
                        error!("failed to construct CBOR value: {}", e);
                        return return_500();
                    },
                };
                vehicles.push(cbor_value);
            }
            let mut cbor_data = Vec::new();
            if let Err(e) = ciborium::into_writer(&vehicles, &mut cbor_data) {
                error!("failed to serialize vehicles to CBOR: {}", e);
                return return_500();
            }
            (cbor_data, "application/cbor")
        },
    };

    Response::builder()
        .status(200)
        .header("Content-Type", content_type)
        .body(Full::new(Bytes::from(data)))
        .unwrap_or_else(|_| return_500())
}

#[instrument(skip_all)]
async fn handle_add_edit(_remote_addr: SocketAddr, request: Request<Incoming>, edit: bool) -> Response<Full<Bytes>> {
    let query_pairs = match get_query_pairs(request.uri().query()) {
        Some(qp) => qp,
        None => return return_400("invalid UTF-8 in query"),
    };

    let edit_id_opt = if edit {
        let edit_id_str_opt = query_pairs.iter()
            .filter(|(k, _v)| k == "id")
            .map(|(_k, v)| v)
            .flatten()
            .last();
        let edit_id_str = match edit_id_str_opt {
            Some(eis) => eis,
            None => return return_400("missing parameter 'id'"),
        };
        let edit_id: i64 = match edit_id_str.parse() {
            Ok(ei) => ei,
            Err(_) => return return_400("invalid parameter value for 'id'"),
        };
        Some(edit_id)
    } else {
        None
    };

    let mut db_conn = match db_connect().await {
        Some(dbc) => dbc,
        None => return return_500(),
    };

    let (base_path, allowed_veh_classes, allowed_power_sources)= {
        let config = CONFIG
            .get().expect("CONFIG not set?!");
        (
            &config.http.base_path,
            config.value_sets.vehicle_classes.clone(),
            config.value_sets.power_sources.clone(),
        )
    };
    if request.method() == Method::GET {
        let template = if let Some(edit_id) = edit_id_opt {
            // find entry
            let found_rows_res = db_conn.query(
                "
                    SELECT
                        company, veh_number, type_code, veh_class,
                        in_service_since, out_of_service_since, manufacturer, depot,
                        other_data
                    FROM
                        bimdb.bims
                    WHERE
                        id = $1
                ",
                &[&edit_id],
            ).await;
            let found_rows = match found_rows_res {
                Ok(fr) => fr,
                Err(e) => {
                    error!("failed to obtain existing vehicle {}: {}", edit_id, e);
                    return return_500();
                },
            };
            if found_rows.len() == 0 {
                return return_400("failed to find this vehicle");
            }

            let company: String = found_rows[0].get(0);
            let veh_number: String = found_rows[0].get(1);
            let type_code: String = found_rows[0].get(2);
            let vehicle_class: String = found_rows[0].get(3);
            let in_service_since: Option<String> = found_rows[0].get(4);
            let out_of_service_since: Option<String> = found_rows[0].get(5);
            let manufacturer: Option<String> = found_rows[0].get(6);
            let depot: Option<String> = found_rows[0].get(7);
            let other_data: serde_json::Value = found_rows[0].get(8);

            let power_source_rows_res = db_conn.query(
                "
                    SELECT
                        power_source
                    FROM
                        bimdb.power_sources
                    WHERE
                        bim_id = $1
                ",
                &[&edit_id],
            ).await;
            let power_source_rows = match power_source_rows_res {
                Ok(fr) => fr,
                Err(e) => {
                    error!("failed to obtain power sources for existing vehicle {}: {}", edit_id, e);
                    return return_500();
                },
            };
            let mut power_sources = BTreeSet::new();
            for row in power_source_rows {
                let power_source: String = row.get(0);
                power_sources.insert(power_source);
            }

            AddEditTemplate {
                base_path: base_path.clone(),
                edit_id: Some(edit_id),
                company: Some(company),
                veh_number: Some(veh_number),
                type_code: Some(type_code),
                veh_class: Some(vehicle_class),
                power_sources,
                in_service_since,
                out_of_service_since,
                manufacturer,
                depot,
                other_data: Some(serde_json::to_string_pretty(&other_data).expect("failed to stringify other data JSON")),
                allowed_veh_classes,
                allowed_power_sources,
            }
        } else {
            AddEditTemplate {
                base_path: base_path.clone(),
                edit_id: None,
                company: None,
                veh_number: None,
                type_code: None,
                veh_class: None,
                power_sources: BTreeSet::new(),
                in_service_since: None,
                out_of_service_since: None,
                manufacturer: None,
                depot: None,
                other_data: None,
                allowed_veh_classes,
                allowed_power_sources,
            }
        };

        let template_text = template.render()
            .expect("failed to render template");
        Response::builder()
            .status(200)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(Full::new(Bytes::from(template_text)))
            .unwrap_or_else(|_| return_500())
    } else if request.method() == Method::POST {
        let (_request_head, request_body) = request.into_parts();
        let request_bytes = match request_body.collect().await {
            Ok(rb) => rb.to_bytes(),
            Err(e) => {
                error!("failed to read request bytes: {}", e);
                return return_500();
            },
        };

        let form_values: ValueMultiset<Cow<str>, Cow<str>> = form_urlencoded::parse(&request_bytes)
            .collect();

        let company = match form_values.get_last("company") {
            Some(c) => if c.len() == 0 {
                return return_400("field 'company' must not be empty");
            } else {
                c
            },
            None => return return_400("field 'company' is required"),
        };
        let vehicle_number = match form_values.get_last("veh-number") {
            Some(c) => if c.len() == 0 {
                return return_400("field 'veh-number' must not be empty");
            } else {
                c
            },
            None => return return_400("field 'veh-number' is required"),
        };
        let type_code = match form_values.get_last("type-code") {
            Some(c) => if c.len() == 0 {
                return return_400("field 'type-code' must not be empty");
            } else {
                c
            },
            None => return return_400("field 'type-code' is required"),
        };
        let vehicle_class = match form_values.get_last("veh-class") {
            Some(c) => if c.len() == 0 {
                return return_400("field 'veh-class' must not be empty");
            } else {
                c
            },
            None => return return_400("field 'veh-class' is required"),
        };
        let mut power_sources = BTreeSet::new();
        for power_source_value in form_values.get_list_or_empty("power-source") {
            for line in power_source_value.split("\n") {
                let trimmed_line = line.trim();
                if trimmed_line.len() == 0 {
                    continue;
                }
                power_sources.insert(trimmed_line.to_owned());
            }
        }
        let in_service_since = form_values.get_last("in-service-since")
            .and_then(|c| if c.len() == 0 { None } else { Some(c) });
        let out_of_service_since = form_values.get_last("out-of-service-since")
            .and_then(|c| if c.len() == 0 { None } else { Some(c) });
        let manufacturer = form_values.get_last("manufacturer")
            .and_then(|c| if c.len() == 0 { None } else { Some(c) });
        let depot = form_values.get_last("depot")
            .and_then(|c| if c.len() == 0 { None } else { Some(c) });
        let other_data_string = match form_values.get_last("other-data") {
            Some(c) => if c.len() == 0 {
                return return_400("field 'other-data' must not be empty");
            } else {
                c
            },
            None => return return_400("field 'other-data' is required"),
        };
        let other_data: serde_json::Value = match serde_json::from_str(&other_data_string) {
            Ok(od) => od,
            Err(e) => {
                error!("failed to parse other data: {}", e);
                return return_400("field 'other-data' is not valid JSON");
            },
        };
        if !other_data.is_object() {
            return return_400("field 'other-data' does not contain a JSON object");
        }

        let value_sets = {
            let config = CONFIG
                .get().expect("CONFIG not set?!");
            &config.value_sets
        };
        if value_sets.vehicle_classes.len() > 0 {
            if !value_sets.vehicle_classes.contains(vehicle_class.as_ref()) {
                return return_400("field 'veh-class' is not one of the allowed values");
            }
        }
        if value_sets.power_sources.len() > 0 {
            for power_source in &power_sources {
                if !value_sets.power_sources.contains(power_source) {
                    return return_400("one of the 'power-source' values is not one of the allowed values");
                }
            }
        }

        let transact = match db_conn.transaction().await {
            Ok(t) => t,
            Err(e) => {
                error!("failed to begin database transaction: {}", e);
                return return_500();
            },
        };

        let bim_id = if let Some(edit_id) = edit_id_opt {
            let update_res = transact.execute(
                "
                    UPDATE bimdb.bims
                    SET
                        company = $1,
                        veh_number = $2,
                        type_code = $3,
                        veh_class = $4,
                        in_service_since = $5,
                        out_of_service_since = $6,
                        manufacturer = $7,
                        depot = $8,
                        other_data = $9
                    WHERE
                        id = $10
                ",
                &[
                    &company, &vehicle_number, &type_code, &vehicle_class,
                    &in_service_since, &out_of_service_since, &manufacturer, &depot,
                    &other_data,
                    &edit_id,
                ],
            ).await;
            if let Err(e) = update_res {
                error!("failed to update vehicle {}: {}", edit_id, e);
                return return_500();
            }
            edit_id
        } else {
            let insert_res = transact.query_one(
                "
                    INSERT INTO bimdb.bims
                        (
                            id,
                            company, veh_number, type_code, veh_class,
                            in_service_since, out_of_service_since, manufacturer, depot,
                            other_data
                        )
                    VALUES
                        (
                            DEFAULT,
                            $1, $2, $3, $4,
                            $5, $6, $7, $8,
                            $9
                        )
                    RETURNING id
                ",
                &[
                    &company, &vehicle_number, &type_code, &vehicle_class,
                    &in_service_since, &out_of_service_since, &manufacturer, &depot,
                    &other_data,
                ],
            ).await;
            match insert_res {
                Ok(row) => {
                    let inserted_id: i64 = row.get(0);
                    inserted_id
                },
                Err(e) => {
                    error!("failed to insert vehicle: {}", e);
                    return return_500();
                },
            }
        };

        // replace power sources
        let delete_power_sources_res = transact.execute(
            "DELETE FROM bimdb.power_sources WHERE bim_id = $1",
            &[&bim_id],
        ).await;
        if let Err(e) = delete_power_sources_res {
            error!("failed to delete power sources for {}: {}", bim_id, e);
            return return_500();
        }

        let insert_stmt_res = transact.prepare(
            "INSERT INTO bimdb.power_sources (bim_id, power_source) VALUES ($1, $2)",
        ).await;
        let insert_stmt = match insert_stmt_res {
            Ok(is) => is,
            Err(e) => {
                error!("failed to prepare insert-power-source statement: {}", e);
                return return_500();
            },
        };
        for power_source in &power_sources {
            if let Err(e) = transact.execute(&insert_stmt, &[&bim_id, &power_source.as_str()]).await {
                error!("failed to insert power source {:?} for {}: {}", power_source, bim_id, e);
                return return_500();
            }
        }

        if let Err(e) = transact.commit().await {
            error!("failed to commit vehicle insertion/editing transaction: {}", e);
            return return_500();
        }

        let base_path_or_slash = if base_path.len() == 0 { "/" } else { base_path };
        Response::builder()
            .status(302)
            .header("Location", base_path_or_slash)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(Full::new(Bytes::from("redirecting...")))
            .unwrap_or_else(|_| return_500())
    } else {
        return_405(request.method(), &[Method::GET, Method::POST])
    }
}

#[instrument(skip_all)]
async fn handle_delete(_remote_addr: SocketAddr, request: Request<Incoming>) -> Response<Full<Bytes>> {
    let query_pairs = match get_query_pairs(request.uri().query()) {
        Some(qp) => qp,
        None => return return_400("invalid UTF-8 in query"),
    };

    if request.method() != Method::POST {
        return return_405(request.method(), &[Method::POST]);
    }

    let delete_id_str_opt = query_pairs.iter()
        .filter(|(k, _v)| k == "id")
        .map(|(_k, v)| v)
        .flatten()
        .last();
    let delete_id_str = match delete_id_str_opt {
        Some(eis) => eis,
        None => return return_400("missing parameter 'id'"),
    };
    let delete_id: i64 = match delete_id_str.parse() {
        Ok(ei) => ei,
        Err(_) => return return_400("invalid parameter value for 'id'"),
    };

    let db_conn = match db_connect().await {
        Some(dbc) => dbc,
        None => return return_500(),
    };

    // delete entry
    let affected_rows_res = db_conn.execute(
        "DELETE FROM bimdb.bims WHERE id = $1",
        &[&delete_id],
    ).await;
    let affected_rows = match affected_rows_res {
        Ok(ar) => ar,
        Err(e) => {
            error!("failed to delete vehicle {}: {}", delete_id, e);
            return return_500();
        },
    };
    if affected_rows == 0 {
        return return_400("failed to find this vehicle");
    }

    let base_path = &CONFIG.get().expect("CONFIG not set?!")
        .http.base_path;
    let base_path_or_slash = if base_path.len() == 0 { "/" } else { base_path };
    Response::builder()
        .status(302)
        .header("Location", base_path_or_slash)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from("redirecting...")))
        .unwrap_or_else(|_| return_500())
}

async fn handle_couplings(_remote_addr: SocketAddr, request: Request<Incoming>) -> Response<Full<Bytes>> {
    if request.method() != Method::GET {
        return return_405(request.method(), &[Method::GET]);
    }

    let db_conn = match db_connect().await {
        Some(dbc) => dbc,
        None => return return_500(),
    };

    // obtain couplings
    let coupling_rows_res = db_conn.query(
        "
            SELECT
                c.id, JSONB_AGG(JSONB_BUILD_OBJECT('id', b.id, 'veh_number', b.veh_number) ORDER BY cb.position) vehicles
            FROM
                bimdb.couplings c
                INNER JOIN bimdb.coupling_bims cb ON cb.coupling_id = c.id
                INNER JOIN bimdb.bims b ON b.id = cb.bim_id
            GROUP BY
                c.id
            ORDER BY
                c.id
        ",
        &[],
    ).await;
    let coupling_rows = match coupling_rows_res {
        Ok(vr) => vr,
        Err(e) => {
            error!("failed to obtain vehicle rows: {}", e);
            return return_500();
        },
    };

    let mut couplings = Vec::new();
    for row in coupling_rows {
        let id: i64 = row.get(0);
        let vehicles_json: serde_json::Value = row.get(1);

        let vehicles: Vec<CouplingVehiclePart> = serde_json::from_value(vehicles_json)
            .expect("coupling not deserializable into CouplingVehiclePart");

        couplings.push(CouplingPart {
            id,
            vehicles,
        })
    }

    let config = CONFIG.get().expect("CONFIG not set?!");
    let template = CouplingListTemplate {
        base_path: config.http.base_path.clone(),
        couplings,
    };
    let template_text = template.render()
        .expect("failed to render template");
    Response::builder()
        .status(200)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Full::new(Bytes::from(template_text)))
        .unwrap_or_else(|_| return_500())
}

#[instrument(skip_all)]
async fn handle_coupling_add_edit(_remote_addr: SocketAddr, request: Request<Incoming>, edit: bool) -> Response<Full<Bytes>> {
    let query_pairs = match get_query_pairs(request.uri().query()) {
        Some(qp) => qp,
        None => return return_400("invalid UTF-8 in query"),
    };

    let edit_id_opt = if edit {
        let edit_id_str_opt = query_pairs.iter()
            .filter(|(k, _v)| k == "id")
            .map(|(_k, v)| v)
            .flatten()
            .last();
        let edit_id_str = match edit_id_str_opt {
            Some(eis) => eis,
            None => return return_400("missing parameter 'id'"),
        };
        let edit_id: i64 = match edit_id_str.parse() {
            Ok(ei) => ei,
            Err(_) => return return_400("invalid parameter value for 'id'"),
        };
        Some(edit_id)
    } else {
        None
    };

    let mut db_conn = match db_connect().await {
        Some(dbc) => dbc,
        None => return return_500(),
    };

    let mut company_to_vehicles = BTreeMap::new();
    let vehicle_rows = match db_conn.query("SELECT company, veh_number FROM bimdb.bims", &[]).await {
        Ok(r) => r,
        Err(e) => {
            error!("failed to obtain list of uncoupled vehicles: {}", e);
            return return_500();
        },
    };
    for row in vehicle_rows {
        let company: String = row.get(0);
        let veh_number: String = row.get(1);

        company_to_vehicles
            .entry(company)
            .or_insert_with(|| BTreeSet::new())
            .insert(veh_number);
    }

    let base_path = &CONFIG
        .get().expect("CONFIG not set?!")
        .http.base_path;
    if request.method() == Method::GET {
        let template = if let Some(edit_id) = edit_id_opt {
            // find coupling
            let found_rows_res = db_conn.query(
                "SELECT id FROM bimdb.couplings WHERE id = $1",
                &[&edit_id],
            ).await;
            let found_rows = match found_rows_res {
                Ok(fr) => fr,
                Err(e) => {
                    error!("failed to obtain existing coupling {}: {}", edit_id, e);
                    return return_500();
                },
            };
            if found_rows.len() == 0 {
                return return_400("failed to find this coupling");
            }

            // get coupling vehicles
            let vehicle_rows_res = db_conn.query(
                "
                    SELECT b.company, b.veh_number
                    FROM bimdb.coupling_bims cb
                    INNER JOIN bimdb.bims b ON b.id = cb.bim_id
                    WHERE cb.coupling_id = $1
                    ORDER BY cb.position
                ",
                &[&edit_id],
            ).await;
            let vehicle_rows = match vehicle_rows_res {
                Ok(vr) => vr,
                Err(e) => {
                    error!("failed to obtain vehicles of existing coupling {}: {}", edit_id, e);
                    return return_500();
                },
            };
            let mut company = None;
            let mut vehicles = Vec::new();
            for vehicle_row in vehicle_rows {
                let veh_company: String = vehicle_row.get(0);
                let veh_number: String = vehicle_row.get(1);

                company = Some(veh_company);
                vehicles.push(veh_number);
            };

            CouplingAddEditTemplate {
                base_path: base_path.clone(),
                edit_id: Some(edit_id),
                company_to_vehicles,
                company,
                vehicles,
            }
        } else {
            CouplingAddEditTemplate {
                base_path: base_path.clone(),
                edit_id: None,
                company_to_vehicles,
                company: None,
                vehicles: Vec::with_capacity(0),
            }
        };

        let template_text = template.render()
            .expect("failed to render template");
        Response::builder()
            .status(200)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(Full::new(Bytes::from(template_text)))
            .unwrap_or_else(|_| return_500())
    } else if request.method() == Method::POST {
        let (_request_head, request_body) = request.into_parts();
        let request_bytes = match request_body.collect().await {
            Ok(rb) => rb.to_bytes(),
            Err(e) => {
                error!("failed to read request bytes: {}", e);
                return return_500();
            },
        };

        let form_values: HashMap<Cow<str>, Cow<str>> = form_urlencoded::parse(&request_bytes)
            .collect();

        let company = match form_values.get("company") {
            Some(c) => if c.len() == 0 {
                return return_400("field 'company' must not be empty");
            } else {
                c
            },
            None => return return_400("field 'company' is required"),
        };

        let vehicles_str = match form_values.get("vehicles") {
            Some(c) => if c.len() == 0 {
                return return_400("field 'vehicles' must not be empty");
            } else {
                c
            },
            None => return return_400("field 'vehicles' is required"),
        };
        let vehicle_numbers: Vec<&str> = vehicles_str.split('\n')
            .map(|veh| veh.trim())
            .filter(|veh| veh.len() > 0)
            .collect();

        // ensure that all vehicles exist
        let select_vehicle_stmt_res = db_conn.prepare("SELECT id FROM bimdb.bims WHERE company = $1 AND veh_number = $2").await;
        let select_vehicle_stmt = match select_vehicle_stmt_res {
            Ok(svs) => svs,
            Err(e) => {
                error!("failed to prepare select-bim statement: {}", e);
                return return_500();
            },
        };

        let mut vehicle_ids = Vec::with_capacity(vehicle_numbers.len());
        let mut unknown_vehicle_numbers = Vec::with_capacity(vehicle_numbers.len());
        for vehicle_number in vehicle_numbers {
            let row = match db_conn.query_opt(&select_vehicle_stmt, &[&company.as_ref(), &vehicle_number]).await {
                Ok(Some(row)) => row,
                Ok(None) => {
                    unknown_vehicle_numbers.push(vehicle_number);
                    continue;
                },
                Err(e) => {
                    error!("error querying ID of bim {:?} of company {:?}: {}", vehicle_number, company, e);
                    return return_500();
                },
            };

            let vehicle_id: i64 = row.get(0);
            vehicle_ids.push(vehicle_id);
        }
        if unknown_vehicle_numbers.len() > 0 {
            let mut error_message = "unknown vehicle numbers:".to_owned();
            for uvn in unknown_vehicle_numbers {
                error_message.push_str(uvn);
            }
            return return_400(&error_message);
        }

        let db_txn = match db_conn.transaction().await {
            Ok(dt) => dt,
            Err(e) => {
                error!("failed to create transaction to add/update coupling: {}", e);
                return return_500();
            },
        };
        let insert_stmt = match db_txn.prepare("INSERT INTO bimdb.coupling_bims (bim_id, coupling_id, position) VALUES ($1, $2, $3)").await {
            Ok(is) => is,
            Err(e) => {
                error!("failed to create insert-coupling-bim statement: {}", e);
                return return_500();
            },
        };

        let coupling_id = if let Some(edit_id) = edit_id_opt {
            // delete (and then reinsert) entries
            if let Err(e) = db_txn.execute("DELETE FROM bimdb.coupling_bims WHERE coupling_id = $1", &[&edit_id]).await {
                error!("failed to delete bims of coupling {}: {}", edit_id, e);
                return return_500();
            }

            edit_id
        } else {
            // add new coupling
            let insert_row = match db_txn.query_one("INSERT INTO bimdb.couplings (id) VALUES (DEFAULT) RETURNING id", &[]).await {
                Ok(r) => r,
                Err(e) => {
                    error!("error inserting new coupling: {}", e);
                    return return_500();
                },
            };
            insert_row.get(0)
        };

        for (i, vehicle_id) in vehicle_ids.into_iter().enumerate() {
            let position: i64 = (i + 1).try_into().unwrap();

            if let Err(e) = db_txn.execute(&insert_stmt, &[&vehicle_id, &coupling_id, &position]).await {
                error!("failed to insert bim {} into coupling {} at position {}: {}", vehicle_id, coupling_id, position, e);
                return return_500();
            }
        }

        if let Err(e) = db_txn.commit().await {
            error!("failed to commit insertion/replacement of vehicles in coupling {}: {}", coupling_id, e);
            return return_500();
        }

        let redirect_path = format!("{}/couplings", base_path);
        Response::builder()
            .status(302)
            .header("Location", &redirect_path)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(Full::new(Bytes::from("redirecting...")))
            .unwrap_or_else(|_| return_500())
    } else {
        return_405(request.method(), &[Method::GET, Method::POST])
    }
}

#[instrument(skip_all)]
async fn handle_coupling_delete(_remote_addr: SocketAddr, request: Request<Incoming>) -> Response<Full<Bytes>> {
    let query_pairs = match get_query_pairs(request.uri().query()) {
        Some(qp) => qp,
        None => return return_400("invalid UTF-8 in query"),
    };

    if request.method() != Method::POST {
        return return_405(request.method(), &[Method::POST]);
    }

    let delete_id_str_opt = query_pairs.iter()
        .filter(|(k, _v)| k == "id")
        .map(|(_k, v)| v)
        .flatten()
        .last();
    let delete_id_str = match delete_id_str_opt {
        Some(eis) => eis,
        None => return return_400("missing parameter 'id'"),
    };
    let delete_id: i64 = match delete_id_str.parse() {
        Ok(ei) => ei,
        Err(_) => return return_400("invalid parameter value for 'id'"),
    };

    let mut db_conn = match db_connect().await {
        Some(dbc) => dbc,
        None => return return_500(),
    };
    let db_txn = match db_conn.transaction().await {
        Ok(t) => t,
        Err(e) => {
            error!("failed to create database transaction: {}", e);
            return return_500();
        },
    };

    // delete vehicles
    let affected_rows_res = db_txn.execute(
        "DELETE FROM bimdb.coupling_bims WHERE coupling_id = $1",
        &[&delete_id],
    ).await;
    if let Err(e) = affected_rows_res {
        error!("failed to delete coupling {} vehicles: {}", delete_id, e);
        return return_500();
    };

    // delete coupling
    let affected_rows_res = db_txn.execute(
        "DELETE FROM bimdb.couplings WHERE id = $1",
        &[&delete_id],
    ).await;
    let affected_rows = match affected_rows_res {
        Ok(ar) => ar,
        Err(e) => {
            error!("failed to delete coupling {}: {}", delete_id, e);
            return return_500();
        },
    };
    if affected_rows == 0 {
        return return_400("failed to find this coupling");
    }

    if let Err(e) = db_txn.commit().await {
        error!("failed to commit coupling deletion transaction: {}", e);
        return return_500();
    }

    let base_path = &CONFIG.get().expect("CONFIG not set?!")
        .http.base_path;
    let redirect_path = format!("{}/couplings", base_path);
    Response::builder()
        .status(302)
        .header("Location", &redirect_path)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from("redirecting...")))
        .unwrap_or_else(|_| return_500())
}

#[instrument(skip(request))]
async fn handle_request(remote_addr: SocketAddr, request: Request<Incoming>) -> Response<Full<Bytes>> {
    // get base path parts from config
    let base_path = &CONFIG
        .get().expect("CONFIG not set?!")
        .http.base_path;
    let base_path_parts = match path_to_parts(&base_path, true) {
        Some(bpp) => bpp,
        None => {
            error!("failed to split CONFIG.http.base_path {:?} into parts", base_path);
            return return_500();
        },
    };

    // get URL path parts
    let uri_path_parts = match path_to_parts(request.uri().path(), true) {
        Some(upp) => upp,
        None => {
            warn!("failed to split URI path {:?} into parts", request.uri().path());
            return return_400("invalid URI path");
        }
    };

    let path_parts = match strip_path_prefix(&uri_path_parts, &base_path_parts) {
        Some(pp) => pp,
        None => return return_400("URI outside of base path"),
    };

    if path_parts.len() == 0 || (path_parts.len() == 1 && path_parts[0].len() == 0) {
        // "/"
        handle_index(remote_addr, request).await
    } else if path_parts.len() == 1 {
        match path_parts[0].as_ref() {
            "json" => handle_export(remote_addr, request, ExportFormat::Json).await,
            "cbor" => handle_export(remote_addr, request, ExportFormat::Cbor).await,
            "add" => handle_add_edit(remote_addr, request, false).await,
            "edit" => handle_add_edit(remote_addr, request, true).await,
            "delete" => handle_delete(remote_addr, request).await,
            "couplings" => handle_couplings(remote_addr, request).await,
            "coupling-add" => handle_coupling_add_edit(remote_addr, request, false).await,
            "coupling-edit" => handle_coupling_add_edit(remote_addr, request, true).await,
            "coupling-delete" => handle_coupling_delete(remote_addr, request).await,
            _ => return_404(),
        }
    } else if path_parts.len() == 2 && path_parts[0] == "static" && STATIC_FILE_REGEX.is_match(path_parts[1].as_ref()) {
        let static_path_opt = {
            let config = CONFIG.get().expect("CONFIG not set?!");
            config.http.static_path.as_ref().map(|sp| PathBuf::from(sp))
        };
        let mut static_path = match static_path_opt {
            Some(sp) => sp,
            None => return return_404(),
        };
        static_path.push(path_parts[1].as_ref());

        if !static_path.is_file() {
            return return_404();
        }

        let contents = match std::fs::read(&static_path) {
            Ok(c) => c,
            Err(e) => {
                error!("failed to read file {:?}: {}", static_path, e);
                return return_500();
            },
        };
        let content_type = if path_parts[1].ends_with(".css") {
            "text/css"
        } else if path_parts[1].ends_with(".js") {
            "text/javascript"
        } else if path_parts[1].ends_with(".js.map") {
            "application/json"
        } else if path_parts[1].ends_with(".ts") {
            "text/x.typescript"
        } else {
            "application/octet-stream"
        };

        Response::builder()
            .status(200)
            .header("Content-Type", content_type)
            .body(Full::new(Bytes::from(contents)))
            .unwrap_or_else(|_| return_500())
    } else {
        return_404()
    }
}


#[tokio::main]
async fn main() -> ExitCode {
    // enable tracing
    tracing_subscriber::fmt::init();

    // find config path
    let args: Vec<OsString> = std::env::args_os().collect();
    let config_path = if args.len() == 1 {
        PathBuf::from("config.toml")
    } else if args.len() == 2 {
        PathBuf::from(&args[1])
    } else {
        eprintln!("Usage: {:?} [CONFIG.TOML]", args[0]);
        return ExitCode::FAILURE;
    };

    // load config
    let config: Config = {
        let mut f = File::open(&config_path)
            .expect("failed to open config file");
        let mut config_bytes = Vec::new();
        f.read_to_end(&mut config_bytes)
            .expect("failed to read config file");
        let config_string = String::from_utf8(config_bytes)
            .expect("failed to decode config file as UTF-8");
        toml::from_str(&config_string)
            .expect("failed to parse config file as TOML")
    };
    CONFIG.set(config)
        .expect("CONFIG already set?!");
    let config = CONFIG.get()
        .expect("CONFIG not set?!");

    // listen to TCP
    let listener = TcpListener::bind(config.http.listen_socket_addr).await
        .expect("failed to open listening socket");

    loop {
        let (stream, remote_addr) = listener.accept().await
            .expect("failed to accept incoming connection");
        let io = TokioIo::new(stream);
        tokio::task::spawn(async move {
            let serve_result = Builder::new(TokioExecutor::new())
                .http1()
                .http2()
                .serve_connection(io, service_fn(move |req| async move {
                    Ok::<_, Infallible>(handle_request(remote_addr, req).await)
                }))
                .await;
            if let Err(serve_error) = serve_result {
                error!("error serving request from {}: {}", remote_addr, serve_error);
            }
        });
    }
}
