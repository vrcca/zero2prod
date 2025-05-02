BEGIN;
    UPDATE subscriptions
    SET status = 'pending'
    WHERE status IS NULL;
    ALTER TABLE subscriptions ALTER COLUMN status SET NOT NULL;
COMMIT;