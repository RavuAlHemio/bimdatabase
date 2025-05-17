CREATE SCHEMA bimdb;

CREATE SEQUENCE bimdb.seq_bims_id AS bigint;
CREATE SEQUENCE bimdb.seq_couplings_id AS bigint;

CREATE TABLE bimdb.bims
( id bigint NOT NULL DEFAULT nextval('bimdb.seq_bims_id')
, company character varying(256) NOT NULL
, veh_number character varying(256) NOT NULL
, type_code character varying(256) NOT NULL
, veh_class character varying(32) NOT NULL
, in_service_since character varying(32) NULL DEFAULT NULL
, out_of_service_since character varying(32) NULL DEFAULT NULL
, manufacturer character varying(32) NULL DEFAULT NULL
, depot character varying(256) NULL DEFAULT NULL
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
  AND   (depot IS NULL OR length(depot) > 0)
  )
);
CREATE INDEX idx_bims_comp_veh_id ON bimdb.bims (company, veh_number, id);

CREATE TABLE bimdb.couplings
( id bigint NOT NULL DEFAULT nextval('bimdb.seq_couplings_id')
, CONSTRAINT pkey_couplings PRIMARY KEY (id)
);

CREATE TABLE bimdb.coupling_bims
( bim_id bigint NOT NULL
, coupling_id bigint NOT NULL
, position bigint NOT NULL
, CONSTRAINT pkey_coupling_bims PRIMARY KEY (bim_id)
, CONSTRAINT fkey_coupling_bims_couplings FOREIGN KEY (coupling_id) REFERENCES bimdb.couplings (id)
, CONSTRAINT fkey_coupling_bims_bims FOREIGN KEY (bim_id) REFERENCES bimdb.bims (id)
, CONSTRAINT uq_coupling_bims_coupling_id_position UNIQUE (coupling_id, position)
);

CREATE OR REPLACE FUNCTION bimdb.trigger_check_coupling_bims() RETURNS trigger AS $$
DECLARE
  new_company character varying(256);
  existing_company character varying(256);
BEGIN
  -- check that the coupling only contains vehicles from one company
  SELECT b.company INTO existing_company
    FROM bimdb.coupling_bims cb
    INNER JOIN bimdb.bims b ON b.id = cb.bim_id
    WHERE cb.coupling_id = new.coupling_id
    LIMIT 1
  ;
  IF existing_company IS NOT NULL
  THEN
    SELECT b.company INTO new_company
      FROM bimdb.bims b
      WHERE b.id = new.bim_id
    ;
    IF existing_company <> new_company
    THEN
      RAISE EXCEPTION 'adding vehicle from company % to coupling with ID % which has vehicles from company %', new_company, new.coupling_id, existing_company;
    END IF;
  END IF;
  RETURN new;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_check_coupling_bims BEFORE INSERT OR UPDATE ON bimdb.coupling_bims
  FOR EACH ROW EXECUTE FUNCTION bimdb.trigger_check_coupling_bims();

CREATE TABLE bimdb.power_sources
( bim_id bigint NOT NULL
, power_source character varying(256) NOT NULL
, CONSTRAINT pkey_power_sources PRIMARY KEY (bim_id, power_source)
, CONSTRAINT fk_power_sources_bim_id FOREIGN KEY (bim_id) REFERENCES bimdb.bims (id)
, CONSTRAINT ck_power_sources_no_empty_str CHECK
  (     length(power_source) > 0
  )
);

CREATE TABLE bimdb.schema_version
( schema_version bigint NOT NULL
);
INSERT INTO bimdb.schema_version (schema_version) VALUES (4);
