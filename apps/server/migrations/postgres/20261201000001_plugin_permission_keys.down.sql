DELETE FROM role_permissions WHERE length(resource) > 50;
DELETE FROM auth_token_permissions WHERE length(resource) > 50;
ALTER TABLE role_permissions ALTER COLUMN resource TYPE VARCHAR(50);
ALTER TABLE auth_token_permissions ALTER COLUMN resource TYPE VARCHAR(50);
