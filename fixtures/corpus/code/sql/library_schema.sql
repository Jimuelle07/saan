-- Schema for a small public library: books, copies, members and loans.

CREATE TABLE authors (
    author_id   INTEGER PRIMARY KEY,
    full_name   TEXT NOT NULL,
    birth_year  INTEGER
);

CREATE TABLE books (
    book_id     INTEGER PRIMARY KEY,
    isbn        TEXT UNIQUE NOT NULL,
    title       TEXT NOT NULL,
    published   INTEGER,
    genre       TEXT
);

CREATE TABLE book_authors (
    book_id     INTEGER REFERENCES books(book_id) ON DELETE CASCADE,
    author_id   INTEGER REFERENCES authors(author_id),
    PRIMARY KEY (book_id, author_id)
);

-- A library owns several physical copies of the same book.
CREATE TABLE copies (
    copy_id     INTEGER PRIMARY KEY,
    book_id     INTEGER NOT NULL REFERENCES books(book_id),
    branch      TEXT NOT NULL,
    condition   TEXT CHECK (condition IN ('new', 'good', 'worn', 'damaged'))
);

CREATE TABLE members (
    member_id   INTEGER PRIMARY KEY,
    card_number TEXT UNIQUE NOT NULL,
    name        TEXT NOT NULL,
    email       TEXT,
    joined_on   DATE NOT NULL DEFAULT CURRENT_DATE
);

CREATE TABLE loans (
    loan_id     INTEGER PRIMARY KEY,
    copy_id     INTEGER NOT NULL REFERENCES copies(copy_id),
    member_id   INTEGER NOT NULL REFERENCES members(member_id),
    borrowed_on DATE NOT NULL,
    due_on      DATE NOT NULL,
    returned_on DATE
);

CREATE INDEX idx_loans_open ON loans(member_id) WHERE returned_on IS NULL;

-- Overdue items with the borrower's contact details.
CREATE VIEW overdue_loans AS
SELECT m.name, m.email, b.title, l.due_on
FROM loans l
JOIN copies c  ON c.copy_id = l.copy_id
JOIN books b   ON b.book_id = c.book_id
JOIN members m ON m.member_id = l.member_id
WHERE l.returned_on IS NULL AND l.due_on < CURRENT_DATE;
