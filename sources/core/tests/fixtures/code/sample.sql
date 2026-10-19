-- Orders, one row each.
CREATE TABLE orders (
  id INTEGER PRIMARY KEY,
  total INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS shop.lines (
  order_id INTEGER REFERENCES orders (id)
);

-- What is open.
CREATE VIEW open_orders AS
  SELECT id FROM orders WHERE total > 0;

CREATE INDEX orders_total ON orders (total);

CREATE FUNCTION add_one(n integer) RETURNS integer AS $$
  SELECT n + 1;
$$ LANGUAGE sql;

SELECT count(*) FROM orders;
