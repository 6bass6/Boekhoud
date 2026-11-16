-- 0011: which ledger accounts a purchase invoice line may be booked on.
-- Purchase lines are costs (expense accounts) or acquisitions of business assets (class 0 fixed
-- assets). Liabilities such as 1600 Crediteuren, receivables, VAT, bank, equity and revenue are never
-- valid "cost accounts": choosing them silently kept costs out of the profit and loss account.
-- Depreciation (4800) and exchange/interest results (9000, 9100) are booked by the app itself.

ALTER TABLE account ADD COLUMN purchase_allowed INTEGER NOT NULL DEFAULT 0 CHECK (purchase_allowed IN (0,1));
UPDATE account SET purchase_allowed = 1 WHERE account_type = 'expense' AND number NOT IN ('4800', '9000', '9100');
UPDATE account SET purchase_allowed = 1 WHERE number = '0100';
