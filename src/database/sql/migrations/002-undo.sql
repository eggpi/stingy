CREATE TABLE undo_steps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL
);

CREATE TABLE undo_statements (
    undo_step_id INTEGER,
    statement TEXT NOT NULL,
    FOREIGN KEY(undo_step_id) REFERENCES undo_steps(id) ON DELETE CASCADE
);
