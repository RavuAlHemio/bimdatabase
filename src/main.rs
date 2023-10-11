mod config;
mod filters;


use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::convert::Infallible;
use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use askama::Template;
use ciborium::cbor;
use form_urlencoded;
use hyper::{Body, Method, Request, Response};
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use once_cell::sync::Lazy;
use percent_encoding;
use regex::Regex;
use toml;
use tracing::{error, instrument, warn};
use tracing_subscriber;

use crate::config::{CONFIG, Config};


static STATIC_FILE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "^",
    "[A-Za-z0-9_-]+",
    "(?:",
        "[.]",
        "[A-Za-z0-9_-]+",
    ")*",
    "$",
)).expect("failed to compile static file regex"));


struct BimPart {
    pub id: i64,
    pub company: String,
    pub veh_number: String,
    pub type_code: String,
    pub veh_class: String,
    pub in_service_since: Option<String>,
    pub out_of_service_since: Option<String>,
    pub manufacturer: Option<String>,
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    pub companies: BTreeSet<String>,
    pub vehicles: Vec<BimPart>,
    pub base_path: String,
    pub page: i64,
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
    pub in_service_since: Option<String>,
    pub out_of_service_since: Option<String>,
    pub manufacturer: Option<String>,
    pub other_data: Option<String>,
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


fn return_500() -> Response<Body> {
    Response::builder()
        .status(500)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Body::from("500 Internal Server Error"))
        .expect("failed to construct HTTP 500 response")
}
fn return_400(reason: &str) -> Response<Body> {
    let body_string = format!("400 Bad Request: {}", reason);
    Response::builder()
        .status(400)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Body::from(body_string))
        .unwrap_or_else(|_| return_500())
}
fn return_404() -> Response<Body> {
    Response::builder()
        .status(400)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Body::from("404 Not Found"))
        .unwrap_or_else(|_| return_500())
}
fn return_405(method: &Method, allowed_methods: &[Method]) -> Response<Body> {
    let allowed_methods: Vec<&str> = allowed_methods.iter().map(|m| m.as_str()).collect();
    let allowed_methods_string = allowed_methods.join(", ");
    let body_text = format!("unsupported method {}; allowed: {}", method, allowed_methods_string);
    Response::builder()
        .status(405)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Allow", &allowed_methods_string)
        .body(Body::from(body_text))
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
async fn handle_index(_remote_addr: SocketAddr, request: Request<Body>) -> Response<Body> {
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
    const PER_PAGE: i64 = 20;
    let page_str = query_pairs.iter()
        .filter(|(k, _v)| k == "page")
        .map(|(_k, v)| v.as_ref().map(|v2| v2.as_str()))
        .flatten()
        .last()
        .unwrap_or("0");
    let page: i64 = match page_str.parse() {
        Ok(pn) => if pn < 0 {
            return return_400(&format!("'page' must be >= 0"));
        } else {
            pn
        },
        Err(_) => return return_400("invalid 'page'"),
    };
    let vehicle_rows_res = db_conn.query(
        "
            SELECT
                id, company, veh_number, type_code,
                veh_class, in_service_since, out_of_service_since, manufacturer
            FROM
                bimdb.bims
            ORDER BY
                company, veh_number, id
            LIMIT $1 OFFSET $2
        ",
        &[&PER_PAGE, &(page*PER_PAGE)],
    ).await;
    let vehicle_rows = match vehicle_rows_res {
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
        vehicles.push(BimPart {
            id,
            company,
            veh_number,
            type_code,
            veh_class,
            in_service_since,
            out_of_service_since,
            manufacturer,
        })
    }

    let config = CONFIG.get().expect("CONFIG not set?!");
    let template = IndexTemplate {
        companies,
        vehicles,
        base_path: config.http.base_path.clone(),
        page,
    };
    let template_text = template.render()
        .expect("failed to render template");
    Response::builder()
        .status(200)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Body::from(template_text))
        .unwrap_or_else(|_| return_500())
}

#[instrument(skip_all)]
async fn handle_export(_remote_addr: SocketAddr, request: Request<Body>, format: ExportFormat) -> Response<Body> {
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

    // obtain vehicles
    let vehicle_rows_res = db_conn.query(
        "
            SELECT
                veh_number, type_code, veh_class, in_service_since,
                out_of_service_since, manufacturer, other_data
            FROM
                bimdb.bims
            WHERE
                company = $1
            ORDER BY
                veh_number, id
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
            for row in vehicle_rows {
                let veh_number: String = row.get(0);
                let type_code: String = row.get(1);
                let veh_class: String = row.get(2);
                let in_service_since: Option<String> = row.get(3);
                let out_of_service_since: Option<String> = row.get(4);
                let manufacturer: Option<String> = row.get(5);
                let other_data: serde_json::Value = row.get(6);
                vehicles.push(serde_json::json!({
                    "number": veh_number,
                    "vehicle_class": veh_class,
                    "type_code": type_code,
                    "in_service_since": in_service_since,
                    "out_of_service_since": out_of_service_since,
                    "manufacturer": manufacturer,
                    "other_data": other_data,
                    "fixed_coupling": [],
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
            for row in vehicle_rows {
                let veh_number: String = row.get(0);
                let type_code: String = row.get(1);
                let veh_class: String = row.get(2);
                let in_service_since: Option<String> = row.get(3);
                let out_of_service_since: Option<String> = row.get(4);
                let manufacturer: Option<String> = row.get(5);
                let other_data: serde_json::Value = row.get(6);
                let cbor_value_res = cbor!({
                    "number" => veh_number,
                    "vehicle_class" => veh_class,
                    "type_code" => type_code,
                    "in_service_since" => in_service_since,
                    "out_of_service_since" => out_of_service_since,
                    "manufacturer" => manufacturer,
                    "other_data" => other_data,
                    "fixed_coupling" => [],
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
        .body(Body::from(data))
        .unwrap_or_else(|_| return_500())
}

#[instrument(skip_all)]
async fn handle_add(_remote_addr: SocketAddr, request: Request<Body>) -> Response<Body> {
    let base_path = &CONFIG
        .get().expect("CONFIG not set?!")
        .http.base_path;
    if request.method() == Method::GET {
        let template = AddEditTemplate {
            base_path: base_path.clone(),
            edit_id: None,
            company: None,
            veh_number: None,
            type_code: None,
            veh_class: None,
            in_service_since: None,
            out_of_service_since: None,
            manufacturer: None,
            other_data: None,
        };
        let template_text = template.render()
            .expect("failed to render template");
        Response::builder()
            .status(200)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(Body::from(template_text))
            .unwrap_or_else(|_| return_500())
    } else if request.method() == Method::POST {
        let (_request_head, request_body) = request.into_parts();
        let request_bytes_res = hyper::body::to_bytes(request_body).await;
        let request_bytes = match request_bytes_res {
            Ok(rb) => rb,
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
        let vehicle_number = match form_values.get("veh-number") {
            Some(c) => if c.len() == 0 {
                return return_400("field 'veh-number' must not be empty");
            } else {
                c
            },
            None => return return_400("field 'veh-number' is required"),
        };
        let type_code = match form_values.get("type-code") {
            Some(c) => if c.len() == 0 {
                return return_400("field 'type-code' must not be empty");
            } else {
                c
            },
            None => return return_400("field 'type-code' is required"),
        };
        let vehicle_class = match form_values.get("veh-class") {
            Some(c) => if c.len() == 0 {
                return return_400("field 'veh-class' must not be empty");
            } else {
                c
            },
            None => return return_400("field 'veh-class' is required"),
        };
        let in_service_since = form_values.get("in-service-since")
            .and_then(|c| if c.len() == 0 { None } else { Some(c) });
        let out_of_service_since = form_values.get("out-of-service-since")
            .and_then(|c| if c.len() == 0 { None } else { Some(c) });
        let manufacturer = form_values.get("manufacturer")
            .and_then(|c| if c.len() == 0 { None } else { Some(c) });
        let other_data_string = match form_values.get("other-data") {
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
            return return_400("field 'other-data' ooes not contain a JSON object");
        }

        let db_conn = match db_connect().await {
            Some(dbc) => dbc,
            None => return return_500(),
        };
        let insert_res = db_conn.execute(
            "
                INSERT INTO bimdb.bims
                    (
                        id, company, veh_number, type_code,
                        veh_class, in_service_since, out_of_service_since, manufacturer,
                        other_data
                    )
                VALUES
                    (
                        DEFAULT, $1, $2, $3,
                        $4, $5, $6, $7,
                        $8
                    )
            ",
            &[
                &company, &vehicle_number, &type_code,
                &vehicle_class, &in_service_since, &out_of_service_since, &manufacturer,
                &other_data,
            ],
        ).await;
        if let Err(e) = insert_res {
            error!("failed to insert vehicle: {}", e);
            return return_500();
        }

        Response::builder()
            .status(302)
            .header("Location", base_path)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(Body::from("redirecting..."))
            .unwrap_or_else(|_| return_500())
    } else {
        return_405(request.method(), &[Method::GET, Method::POST])
    }
}

#[instrument(skip_all)]
async fn handle_edit(_remote_addr: SocketAddr, request: Request<Body>) -> Response<Body> {
    let query_pairs = match get_query_pairs(request.uri().query()) {
        Some(qp) => qp,
        None => return return_400("invalid UTF-8 in query"),
    };

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

    let db_conn = match db_connect().await {
        Some(dbc) => dbc,
        None => return return_500(),
    };

    let base_path = &CONFIG
        .get().expect("CONFIG not set?!")
        .http.base_path;
    if request.method() == Method::GET {
        // find entry
        let found_rows_res = db_conn.query(
            "
                SELECT
                    company, veh_number, type_code, veh_class,
                    in_service_since, out_of_service_since, manufacturer, other_data
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
        let other_data: serde_json::Value = found_rows[0].get(7);

        let template = AddEditTemplate {
            base_path: base_path.clone(),
            edit_id: Some(edit_id),
            company: Some(company),
            veh_number: Some(veh_number),
            type_code: Some(type_code),
            veh_class: Some(vehicle_class),
            in_service_since,
            out_of_service_since,
            manufacturer,
            other_data: Some(serde_json::to_string_pretty(&other_data).expect("failed to stringify other data JSON")),
        };
        let template_text = template.render()
            .expect("failed to render template");
        Response::builder()
            .status(200)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(Body::from(template_text))
            .unwrap_or_else(|_| return_500())
    } else if request.method() == Method::POST {
        let (_request_head, request_body) = request.into_parts();
        let request_bytes_res = hyper::body::to_bytes(request_body).await;
        let request_bytes = match request_bytes_res {
            Ok(rb) => rb,
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
        let vehicle_number = match form_values.get("veh-number") {
            Some(c) => if c.len() == 0 {
                return return_400("field 'veh-number' must not be empty");
            } else {
                c
            },
            None => return return_400("field 'veh-number' is required"),
        };
        let type_code = match form_values.get("type-code") {
            Some(c) => if c.len() == 0 {
                return return_400("field 'type-code' must not be empty");
            } else {
                c
            },
            None => return return_400("field 'type-code' is required"),
        };
        let vehicle_class = match form_values.get("veh-class") {
            Some(c) => if c.len() == 0 {
                return return_400("field 'veh-class' must not be empty");
            } else {
                c
            },
            None => return return_400("field 'veh-class' is required"),
        };
        let in_service_since = form_values.get("in-service-since")
            .and_then(|c| if c.len() == 0 { None } else { Some(c) });
        let out_of_service_since = form_values.get("out-of-service-since")
            .and_then(|c| if c.len() == 0 { None } else { Some(c) });
        let manufacturer = form_values.get("manufacturer")
            .and_then(|c| if c.len() == 0 { None } else { Some(c) });
        let other_data_string = match form_values.get("other-data") {
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
            return return_400("field 'other-data' ooes not contain a JSON object");
        }

        let db_conn = match db_connect().await {
            Some(dbc) => dbc,
            None => return return_500(),
        };
        let update_res = db_conn.execute(
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
                    other_data = $8
                WHERE
                    id = $9
            ",
            &[
                &company, &vehicle_number, &type_code,
                &vehicle_class, &in_service_since, &out_of_service_since, &manufacturer,
                &other_data,
                &edit_id,
            ],
        ).await;
        if let Err(e) = update_res {
            error!("failed to update vehicle {}: {}", edit_id, e);
            return return_500();
        }

        Response::builder()
            .status(302)
            .header("Location", base_path)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(Body::from("redirecting..."))
            .unwrap_or_else(|_| return_500())
    } else {
        return_405(request.method(), &[Method::GET, Method::POST])
    }
}

#[instrument(skip(request))]
async fn handle_request(remote_addr: SocketAddr, request: Request<Body>) -> Response<Body> {
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
            "add" => handle_add(remote_addr, request).await,
            "edit" => handle_edit(remote_addr, request).await,
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
            .body(Body::from(contents))
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

    // start up server
    let make_service = make_service_fn(|socket: &AddrStream| {
        let remote_addr = socket.remote_addr();
        async move {
            Ok::<_, Infallible>(service_fn(move |req: Request<Body>| async move {
                Ok::<_, Infallible>(
                    handle_request(remote_addr, req).await
                )
            }))
        }
    });

    // serve!
    let server = hyper::Server::bind(&config.http.listen_socket_addr)
        .serve(make_service);
    if let Err(e) = server.await {
        error!("server error: {}", e);
    }

    ExitCode::SUCCESS
}
