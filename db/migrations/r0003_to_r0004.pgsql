CREATE TABLE bimdb.power_sources
( bim_id bigint NOT NULL
, power_source character varying(256) NOT NULL
, CONSTRAINT pkey_power_sources PRIMARY KEY (bim_id, power_source)
, CONSTRAINT fk_power_sources_bim_id FOREIGN KEY (bim_id) REFERENCES bimdb.bims (id)
, CONSTRAINT ck_power_sources_no_empty_str CHECK
  (     length(power_source) > 0
  )
);

UPDATE bimdb.schema_version SET schema_version = 4;
