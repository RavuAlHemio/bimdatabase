ALTER TABLE bimdb.bims ADD COLUMN depot character varying(256) NULL DEFAULT NULL;
ALTER TABLE bimdb.bims DROP CONSTRAINT ck_bims_no_empty_str;
ALTER TABLE bimdb.bims ADD CONSTRAINT ck_bims_no_empty_str CHECK
  (     length(company) > 0
  AND   length(veh_number) > 0
  AND   length(type_code) > 0
  AND   length(veh_class) > 0
  AND   (in_service_since IS NULL OR length(in_service_since) > 0)
  AND   (out_of_service_since IS NULL OR length(out_of_service_since) > 0)
  AND   (manufacturer IS NULL OR length(manufacturer) > 0)
  AND   (depot IS NULL OR length(depot) > 0)
  )
;

UPDATE bimdb.schema_version SET schema_version = 3;
