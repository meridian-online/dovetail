-- Deterministic retail fixture at realistic size, for scoring table roles.
-- Brightfield's stand-in column names; no random(), only range() arithmetic.
-- The expected roles are in answer-key.json.

CREATE TABLE customers AS SELECT
    CAST(i AS INTEGER) AS customer_id,
    'Customer ' || CAST(i AS VARCHAR) AS name,
    'customer' || CAST(i AS VARCHAR) || '@example.com' AS email,
    DATE '2020-01-01' + CAST((i * 3) % 1500 AS INTEGER) AS joined_on
FROM range(1, 401) t(i);

CREATE TABLE stores AS SELECT
    CAST(i AS INTEGER) AS store_id,
    'City' || CAST(i AS VARCHAR) AS city,
    CASE WHEN i % 3 = 0 THEN 'North' WHEN i % 3 = 1 THEN 'South' ELSE 'Central' END AS region
FROM range(1, 13) t(i);

CREATE TABLE categories AS SELECT
    CAST(i AS INTEGER) AS category_id,
    'Category ' || CAST(i AS VARCHAR) AS name
FROM range(1, 9) t(i);

CREATE TABLE products AS SELECT
    'BEV-' || lpad(CAST(i AS VARCHAR), 3, '0') AS sku,
    'Product ' || CAST(i AS VARCHAR) AS name,
    CAST((i % 8) + 1 AS INTEGER) AS category_id,
    CAST(round(2 + (i % 37) * 0.75, 2) AS DOUBLE) AS unit_price
FROM range(1, 121) t(i);

CREATE TABLE transactions AS SELECT
    CAST(i AS INTEGER) AS txn_id,
    CAST((i * 7) % 400 + 1 AS INTEGER) AS customer_id,
    CAST(i % 12 + 1 AS INTEGER) AS store_id,
    'BEV-' || lpad(CAST((i * 13) % 120 + 1 AS VARCHAR), 3, '0') AS sku,
    CAST(i % 5 + 1 AS INTEGER) AS quantity,
    CAST(round(1 + (i % 97) * 1.25, 2) AS DOUBLE) AS amount,
    TIMESTAMP '2024-01-01 00:00:00' + INTERVAL (i * 17) MINUTE AS purchased_at
FROM range(1, 5001) t(i);

CREATE TABLE loyalty_visits AS SELECT
    CAST(i AS INTEGER) AS visit_id,
    CAST((i * 11) % 400 + 1 AS INTEGER) AS customer_id,
    CAST(i % 12 + 1 AS INTEGER) AS store_id,
    DATE '2024-01-01' + CAST(i % 300 AS INTEGER) AS visited_on,
    CAST(i % 50 + 5 AS INTEGER) AS points
FROM range(1, 1501) t(i);

-- Every customer assessed on six month-ends.
CREATE TABLE customer_segments AS SELECT
    CAST(c AS INTEGER) AS customer_id,
    CASE (c + m) % 4 WHEN 0 THEN 'bronze' WHEN 1 THEN 'silver' WHEN 2 THEN 'gold' ELSE 'platinum' END AS segment,
    DATE '2024-01-31' + INTERVAL (m) MONTH AS assessed_on
FROM range(1, 401) a(c), range(0, 6) b(m);

-- Nothing joins to promo_codes.
CREATE TABLE promo_codes AS SELECT
    'PROMO-' || lpad(CAST(i AS VARCHAR), 3, '0') AS code,
    CAST(5 + (i % 6) * 5 AS INTEGER) AS discount_pct,
    DATE '2025-01-01' + CAST(i * 9 AS INTEGER) AS valid_to
FROM range(1, 21) t(i);

-- Partition pair: disjoint supplier_ids, same columns.
CREATE TABLE suppliers_north AS SELECT
    CAST(i AS INTEGER) AS supplier_id,
    'North Supplier ' || CAST(i AS VARCHAR) AS name,
    'UK' AS country
FROM range(1, 31) t(i);
CREATE TABLE suppliers_south AS SELECT
    CAST(i AS INTEGER) AS supplier_id,
    'South Supplier ' || CAST(i AS VARCHAR) AS name,
    'ES' AS country
FROM range(101, 131) t(i);

-- Snapshot pair: the same skus priced in two years.
CREATE TABLE price_list_2024 AS SELECT
    'BEV-' || lpad(CAST(i AS VARCHAR), 3, '0') AS sku,
    CAST(round(2 + (i % 37) * 0.75, 2) AS DOUBLE) AS unit_price
FROM range(1, 101) t(i);
CREATE TABLE price_list_2025 AS SELECT
    'BEV-' || lpad(CAST(i AS VARCHAR), 3, '0') AS sku,
    CAST(round(2.5 + (i % 37) * 0.8, 2) AS DOUBLE) AS unit_price
FROM range(1, 101) t(i);

-- Snapshot pair of members with overlapping ids, and the visits that point at it.
CREATE TABLE members_2024 AS SELECT
    CAST(i AS INTEGER) AS member_id,
    CASE i % 3 WHEN 0 THEN 'basic' WHEN 1 THEN 'plus' ELSE 'premium' END AS tier
FROM range(1, 301) t(i);
CREATE TABLE members_2025 AS SELECT
    CAST(i AS INTEGER) AS member_id,
    CASE i % 4 WHEN 0 THEN 'basic' WHEN 1 THEN 'plus' WHEN 2 THEN 'premium' ELSE 'elite' END AS tier
FROM range(51, 351) t(i);

CREATE TABLE member_visits AS SELECT
    CAST(i AS INTEGER) AS member_visit_id,
    CAST(51 + (i * 7) % 250 AS INTEGER) AS member_id,
    TIMESTAMP '2024-03-01 09:00:00' + INTERVAL (i * 29) MINUTE AS visited_at
FROM range(1, 2001) t(i);
