-- 0003: versioned VAT codes, VAT regime (regular / KOR) and exchange rates.
-- Fiscal rules are data: change them by adding rows with a new valid_from, never by editing code.

CREATE TABLE tax_code (
    code             TEXT NOT NULL,
    valid_from       TEXT NOT NULL CHECK (date(valid_from) = valid_from),
    valid_until      TEXT CHECK (valid_until IS NULL OR date(valid_until) = valid_until),
    name             TEXT NOT NULL,
    kind             TEXT NOT NULL CHECK (kind IN ('sales','purchase','none')),
    -- Rate in basis points (21% = 2100); NULL when no VAT is calculated.
    rate_bp          INTEGER CHECK (rate_bp IS NULL OR rate_bp BETWEEN 0 AND 10000),
    -- Rubriek(en) of the Dutch VAT return, e.g. '1a', '3b', '4b+5b'. NULL = not on the return / to be confirmed.
    rubric           TEXT,
    icp              INTEGER NOT NULL DEFAULT 0 CHECK (icp IN (0,1)),
    reverse_charge   INTEGER NOT NULL DEFAULT 0 CHECK (reverse_charge IN (0,1)),
    -- Only usable under this VAT regime; NULL = any regime.
    regime           TEXT CHECK (regime IS NULL OR regime IN ('regular','kor')),
    note             TEXT,
    PRIMARY KEY (code, valid_from),
    CHECK (valid_until IS NULL OR valid_until >= valid_from)
);

CREATE TABLE vat_regime (
    valid_from       TEXT PRIMARY KEY CHECK (date(valid_from) = valid_from),
    regime           TEXT NOT NULL CHECK (regime IN ('regular','kor')),
    note             TEXT,
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE exchange_rate (
    rate_date            TEXT NOT NULL CHECK (date(rate_date) = rate_date),
    currency             TEXT NOT NULL CHECK (length(currency) = 3),
    -- Units of `currency` per 1 EUR, scaled by 1,000,000 (ECB convention: 1 EUR = 1.0823 USD -> 1082300).
    units_per_eur_micro  INTEGER NOT NULL CHECK (units_per_eur_micro > 0),
    source               TEXT NOT NULL,
    PRIMARY KEY (rate_date, currency, source)
);

-- Concept codes from docs/accounting-rules.md §4. Rubrics marked [TOETSEN] there are to be confirmed.
INSERT INTO tax_code (code, valid_from, name, kind, rate_bp, rubric, icp, reverse_charge, regime, note) VALUES
    ('V21',   '2020-01-01', 'Verkoop hoog tarief',                    'sales',    2100, '1a',    0, 0, 'regular', NULL),
    ('V9',    '2020-01-01', 'Verkoop laag tarief',                    'sales',     900, '1b',    0, 0, 'regular', NULL),
    ('V0EU',  '2020-01-01', 'Dienst aan EU-ondernemer, btw verlegd',  'sales',    NULL, '3b',    1, 1, NULL,      'Ook App Store/Google Play aan Ierse entiteit [TOETSEN]'),
    ('V0NEU', '2020-01-01', 'Dienst aan ondernemer buiten de EU',     'sales',    NULL, NULL,    0, 0, NULL,      'Rubriek te bevestigen [TOETSEN]'),
    ('VKOR',  '2020-01-01', 'Verkoop onder de KOR',                   'sales',    NULL, NULL,    0, 0, 'kor',     NULL),
    ('I21',   '2020-01-01', 'Inkoop NL hoog tarief',                  'purchase', 2100, '5b',    0, 0, 'regular', NULL),
    ('I9',    '2020-01-01', 'Inkoop NL laag tarief',                  'purchase',  900, '5b',    0, 0, 'regular', NULL),
    ('IEU',   '2020-01-01', 'Inkoop dienst uit EU, btw verlegd',      'purchase', 2100, '4b+5b', 0, 1, 'regular', NULL),
    ('INEU',  '2020-01-01', 'Inkoop dienst buiten EU, btw verlegd',   'purchase', 2100, '4a+5b', 0, 1, 'regular', NULL),
    ('GEEN',  '2020-01-01', 'Geen btw / buiten de btw',               'none',     NULL, NULL,    0, 0, NULL,      NULL);
