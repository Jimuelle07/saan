-- Monthly revenue report by region with month-over-month growth.
-- Source tables: orders(order_id, customer_id, ordered_at, status)
--                order_items(order_id, product_id, quantity, unit_price)
--                customers(customer_id, region)

WITH monthly AS (
    SELECT
        date_trunc('month', o.ordered_at)       AS month,
        c.region,
        SUM(oi.quantity * oi.unit_price)        AS revenue,
        COUNT(DISTINCT o.order_id)              AS orders
    FROM orders o
    JOIN order_items oi ON oi.order_id = o.order_id
    JOIN customers c    ON c.customer_id = o.customer_id
    WHERE o.status = 'completed'
      AND o.ordered_at >= date_trunc('year', now()) - interval '1 year'
    GROUP BY 1, 2
)
SELECT
    month,
    region,
    revenue,
    orders,
    ROUND(revenue / NULLIF(orders, 0), 2)                         AS avg_order_value,
    ROUND(
        100.0 * (revenue - LAG(revenue) OVER w) / NULLIF(LAG(revenue) OVER w, 0),
        1
    )                                                             AS mom_growth_pct,
    RANK() OVER (PARTITION BY month ORDER BY revenue DESC)        AS region_rank
FROM monthly
WINDOW w AS (PARTITION BY region ORDER BY month)
ORDER BY month DESC, region_rank;
