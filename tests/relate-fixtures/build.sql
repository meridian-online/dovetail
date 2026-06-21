-- Reproducible relate fixture (spec 2026-06-21-relate-discover-verify-render, ac-01).
-- Four known cases, one per expected outcome. Build into a fresh DuckDB and run
-- dovetail relate against it; expected statuses are in manifest.json.

-- (a) HOLDING FK  → expect: accepted
--     orders.customer_id ⊆ customers.id ; customers.id is unique (a real key).
CREATE TABLE customers (id INTEGER, name VARCHAR);
INSERT INTO customers VALUES (1,'Ada'),(2,'Alan'),(3,'Grace'),(4,'Edsger'),(5,'Barbara');
CREATE TABLE orders (id INTEGER, customer_id INTEGER, total DOUBLE);
INSERT INTO orders VALUES (10,1,9.9),(11,2,4.5),(12,1,3.3),(13,3,8.0),(14,5,1.2);

-- (b) ORPHAN NEAR-MISS  → expect: rejected
--     shipments.order_id has value 99, which is NOT in orders.id → orphan rows.
CREATE TABLE shipments (id INTEGER, order_id INTEGER);
INSERT INTO shipments VALUES (100,10),(101,11),(102,99);

-- (c) COINCIDENTAL BOOLEAN  → expect: rejected
--     widget_flags.active vs gadget_flags.active: exact name match and orphan-free,
--     but the parent is a 2-value boolean — not unique, trivially low cardinality.
CREATE TABLE widget_flags (id INTEGER, active BOOLEAN);
INSERT INTO widget_flags VALUES (1,true),(2,false),(3,true);
CREATE TABLE gadget_flags (id INTEGER, active BOOLEAN);
INSERT INTO gadget_flags VALUES (1,false),(2,true),(3,false),(4,true);

-- (d) AMBIGUOUS MID-CONFIDENCE  → expect: suggested
--     products.category_id ⊆ categories.id (orphan-free) and strong FK-shaped
--     naming + decent cardinality, BUT categories.id has a duplicate (id=3 twice),
--     so the parent is not strictly unique — plausible but not provably a key.
CREATE TABLE categories (id INTEGER, name VARCHAR);
INSERT INTO categories VALUES (1,'tools'),(2,'home'),(3,'garden'),(3,'garden-dup');
CREATE TABLE products (id INTEGER, category_id INTEGER, label VARCHAR);
INSERT INTO products VALUES (1,1,'hammer'),(2,2,'lamp'),(3,3,'rake'),(4,1,'wrench');
