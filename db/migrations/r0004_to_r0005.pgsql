ALTER TABLE bimdb.power_sources DROP CONSTRAINT fk_power_sources_bim_id;
ALTER TABLE bimdb.power_sources ADD CONSTRAINT fk_power_sources_bim_id
  FOREIGN KEY (bim_id) REFERENCES bimdb.bims (id) ON DELETE CASCADE;

UPDATE bimdb.schema_version SET schema_version = 5;
