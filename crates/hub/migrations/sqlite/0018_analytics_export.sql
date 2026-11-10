-- Increment 115 (warehouse.rs) adds Postgres functions only: the ClickHouse exporter runs
-- on Postgres, and SQLite keeps the same tables (0014's board_outbox). This file keeps the
-- two migration directories numbered in step.
SELECT 1;
