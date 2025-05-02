UPDATE subscriptions
SET status = 'pending_confirmation'
WHERE status = 'pending';