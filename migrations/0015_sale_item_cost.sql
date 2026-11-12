-- Cost of the goods sold, snapshotted when the sale is written.
--
-- Without it a margin report would value yesterday's sale at today's HPP, and
-- a purchase at a new price would silently rewrite past profit. The value is
-- the product's most recent production unit cost at the moment of the sale.
--
-- Sales written before this migration have no cost. Their margin cannot be
-- reconstructed without inventing a number, so reports report those lines as
-- cost unknown instead of guessing.

alter table sale_items
    add column unit_cost numeric(18,4) check (unit_cost >= 0);
