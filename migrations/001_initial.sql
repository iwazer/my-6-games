CREATE TABLE IF NOT EXISTS shares (
    id          CHAR(16)    NOT NULL PRIMARY KEY,
    creator     VARCHAR(40) NULL,
    games_json  JSON        NOT NULL,
    created_at  DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    accessed_at DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at  DATETIME    NOT NULL,
    INDEX idx_expires_at (expires_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
