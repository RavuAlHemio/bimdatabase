CREATE SCHEMA bimdb;

CREATE SEQUENCE bimdb.seq_bims_id AS bigint;

CREATE TABLE bimdb.bims
( id bigint NOT NULL DEFAULT nextval('bimdb.seq_bims_id')
, company character varying(256) NOT NULL
, veh_number character varying(256) NOT NULL
, type_code character varying(256) NOT NULL
, veh_class character varying(32) NOT NULL
, in_service_since character varying(32) NULL DEFAULT NULL
, out_of_service_since character varying(32) NULL DEFAULT NULL
, manufacturer character varying(32) NULL DEFAULT NULL
, other_data jsonb NOT NULL
, CONSTRAINT pkey_bims PRIMARY KEY (id)
, CONSTRAINT uq_bims_company_vehnum UNIQUE (company, veh_number)
, CONSTRAINT ck_bims_no_empty_str CHECK
  (     length(company) > 0
  AND   length(veh_number) > 0
  AND   length(type_code) > 0
  AND   length(veh_class) > 0
  AND   (in_service_since IS NULL OR length(in_service_since) > 0)
  AND   (out_of_service_since IS NULL OR length(out_of_service_since) > 0)
  AND   (manufacturer IS NULL OR length(manufacturer) > 0)
  )
);
CREATE INDEX idx_bims_comp_veh_id ON bimdb.bims (company, veh_number, id);

CREATE TABLE bimdb.schema_version
( schema_version bigint NOT NULL
);
INSERT INTO bimdb.schema_version (schema_version) VALUES (1);
