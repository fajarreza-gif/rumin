-- PostgreSQL / Snowflake multi-dialect migration fixture
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE TABLE orders (
    order_id UUID PRIMARY KEY,
    user_id INT REFERENCES users(id),
    amount NUMERIC(10, 2) NOT NULL,
    status VARCHAR(50) DEFAULT 'pending'
);

CREATE VIEW active_user_orders AS
SELECT u.email, o.amount, o.status
FROM users u
JOIN orders o ON u.id = o.user_id
WHERE o.status != 'cancelled';
