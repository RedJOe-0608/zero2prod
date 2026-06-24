-- Add migration script here
CREATE TABLE subscriptions (
id UUID PRIMARY KEY,
name TEXT NOT NULL,
email TEXT NOT NULL UNIQUE,
subscribed_at timestamptz NOT NULL
);