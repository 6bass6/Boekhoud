import { useEffect, useState } from "react";
import { api, fileToBase64, type BankAction, type BankTransaction } from "../api";
import { useLoad } from "../hooks";
import { formatDate, formatEuro } from "../money";

type Filter = "open" | "processed" | "ignored" | "all";

export function Bank({ year }: { year: number }) {
  const accounts = useLoad(() => api.bankAccounts(), []);
  const [accountId, setAccountId] = useState<string>("");
  const [filter, setFilter] = useState<Filter>("open");
  const [selected, setSelected] = useState<string | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [newIban, setNewIban] = useState("");

  useEffect(() => {
    if (!accountId && accounts.data && accounts.data.length > 0) setAccountId(accounts.data[0].id);
  }, [accounts.data, accountId]);

  const txs = useLoad(() => (accountId ? api.bankTransactions(accountId, filter, year) : Promise.resolve([])), [accountId, filter, year]);
  const rec = useLoad(() => (accountId ? api.bankReconciliation(accountId) : Promise.resolve(null)), [accountId]);
  const refresh = () => { txs.reload(); rec.reload(); };

  const importFile = async (file: File) => {
    setBusy(true);
    setMessage(null);
    try {
      const r = await api.importBankFile(file.name, await fileToBase64(file), accountId || null);
      accounts.reload();
      setAccountId(r.account.id);
      setMessage({ ok: true, text: `${r.format}: ${r.result.new_transactions} nieuwe transacties, ${r.result.skipped_duplicates} al aanwezig.` });
      refresh();
    } catch (e) {
      setMessage({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const list = txs.data ?? [];
  const current = list.find((t) => t.id === selected) ?? null;
  return (
    <section className="split">
      <div>
        <header className="row">
          <h2>Bank</h2>
          <span className="row gap">
            {accounts.data && accounts.data.length > 0 && (
              <select value={accountId} onChange={(e) => { setAccountId(e.target.value); setSelected(null); }}>
                {accounts.data.map((a) => <option key={a.id} value={a.id}>{a.name} ({a.iban})</option>)}
              </select>
            )}
            <label className="file-button primary-file">
              {busy ? "Importeren…" : "Afschrift importeren…"}
              <input type="file" accept=".xml,.csv,.txt" hidden onChange={(e) => { const f = e.target.files?.[0]; if (f) importFile(f); e.target.value = ""; }} />
            </label>
          </span>
        </header>
        <p className="muted small">Exporteer bij je bank een CAMT.053-bestand (XML) of CSV. Dubbele transacties uit overlappende exports worden automatisch overgeslagen.</p>
        {message && <p className={message.ok ? "ok" : "error"}>{message.text}</p>}
        {accounts.data?.length === 0 && (
          <div className="box">
            <span className="small">Nog geen bankrekening. Importeer een afschrift (het IBAN wordt herkend) of voeg de rekening toe:</span>
            <span className="row gap">
              <input placeholder="IBAN" value={newIban} onChange={(e) => setNewIban(e.target.value)} />
              <button onClick={() => api.addBankAccount(newIban, "Zakelijke rekening").then((a) => { accounts.reload(); setAccountId(a.id); }).catch((e) => setMessage({ ok: false, text: String(e) }))}>Toevoegen</button>
            </span>
          </div>
        )}
        {rec.data && <ReconciliationBar r={rec.data} />}
        {accountId && (
          <div className="tabs">
            {(["open", "processed", "ignored", "all"] as Filter[]).map((f) => (
              <button key={f} className={filter === f ? "active" : ""} onClick={() => setFilter(f)}>
                {{ open: "Te verwerken", processed: "Verwerkt", ignored: "Genegeerd", all: "Alles" }[f]}
              </button>
            ))}
          </div>
        )}
        {txs.error && <p className="error">{txs.error}</p>}
        {accountId && txs.data && list.length === 0 && <p className="muted">Geen transacties{filter === "open" ? " om te verwerken" : ""}.</p>}
        {list.length > 0 && (
          <table className="clickable">
            <thead><tr><th>Datum</th><th>Tegenpartij / omschrijving</th><th className="num">Bedrag</th><th>Boeking</th></tr></thead>
            <tbody>
              {list.map((t) => (
                <tr key={t.id} className={selected === t.id ? "selected" : ""} onClick={() => setSelected(t.id)}>
                  <td>{formatDate(t.booking_date)}</td>
                  <td className="desc">{t.counterparty_name ?? "—"}<div className="muted small">{t.description}</div></td>
                  <td className={"num " + (t.amount_cents < 0 ? "" : "ok")}>{formatEuro(t.amount_cents)}</td>
                  <td>{t.entry_number ?? (t.state === "ignored" ? <span className="tag">genegeerd</span> : "")}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      {current && <TransactionPanel key={current.id + current.state} t={current} onChanged={refresh} />}
    </section>
  );
}

function ReconciliationBar({ r }: { r: NonNullable<Awaited<ReturnType<typeof api.bankReconciliation>>> }) {
  if (r.statement_date === null) {
    return <p className="small muted">Saldo in administratie: {formatEuro(r.ledger_balance_cents)} · {r.open_transactions} te verwerken.</p>;
  }
  const ok = r.difference_cents === 0;
  return (
    <div className={"box " + (ok ? "" : "warn-box")}>
      <span className="small">
        Bankafschrift {formatDate(r.statement_date)}: <strong>{formatEuro(r.statement_balance_cents ?? 0)}</strong> ·
        administratie op die datum: <strong>{formatEuro(r.ledger_balance_at_statement_cents ?? 0)}</strong> ·
        {ok ? <span className="ok"> sluit aan ✓</span> : <span className="warning"> verschil {formatEuro(r.difference_cents ?? 0)}</span>}
        {r.open_transactions > 0 && ` · ${r.open_transactions} transacties (${formatEuro(r.open_sum_cents)}) nog te verwerken`}
      </span>
    </div>
  );
}

function TransactionPanel({ t, onChanged }: { t: BankTransaction; onChanged: () => void }) {
  const suggestions = useLoad(() => (t.state === "open" ? api.bankSuggestions(t.id) : Promise.resolve([])), [t.id, t.state]);
  const [error, setError] = useState<string | null>(null);
  const [manual, setManual] = useState<"account" | "private" | "ignore" | "invoice" | null>(null);

  const process = (action: BankAction) => {
    setError(null);
    api.processBankTransaction(t.id, action).then(onChanged).catch((e) => setError(String(e)));
  };

  return (
    <aside className="panel">
      <h3 className={t.amount_cents < 0 ? "" : "ok"}>{formatEuro(t.amount_cents)}</h3>
      <p>{t.counterparty_name ?? "—"}<br /><span className="muted small">{formatDate(t.booking_date)}{t.counterparty_iban && ` · ${t.counterparty_iban}`}</span></p>
      <p className="small">{t.description}</p>
      {error && <p className="error">{error}</p>}
      {t.state === "processed" && <p className="ok small">Verwerkt in boeking {t.entry_number}. Ongedaan maken: tegenboek die boeking bij Boekingen.</p>}
      {t.state === "ignored" && (
        <p className="small">Genegeerd: {t.ignore_reason} <button onClick={() => api.reopenBankTransaction(t.id).then(onChanged).catch((e) => setError(String(e)))}>Heropenen</button></p>
      )}
      {t.state === "open" && (
        <>
          {suggestions.data && suggestions.data.length > 0 && (
            <>
              <h4>Voorstellen</h4>
              {suggestions.data.map((s, i) => (
                <div key={i} className="suggestion">
                  <span className="small">{s.label}</span>
                  <span className="row gap">
                    <span className={"tag " + (s.confidence >= 85 ? "state-paid" : s.confidence >= 60 ? "state-partially_paid" : "")}>{s.confidence}%</span>
                    <button className={i === 0 ? "primary" : ""} onClick={() => process(s.action)}>Verwerken</button>
                  </span>
                </div>
              ))}
            </>
          )}
          <h4>Zelf verwerken</h4>
          <div className="row gap wrap">
            <button onClick={() => setManual("account")}>Op rekening boeken</button>
            <button onClick={() => setManual("invoice")}>Aan factuur koppelen</button>
            <button onClick={() => setManual("private")}>Privé</button>
            <button onClick={() => setManual("ignore")}>Negeren</button>
          </div>
          {manual === "account" && <AccountForm t={t} onSubmit={process} />}
          {manual === "invoice" && <InvoicePicker t={t} onSubmit={process} />}
          {manual === "private" && (
            <div className="box">
              <span className="small">{t.amount_cents > 0 ? "Privé-storting (0510)" : "Privé-opname (0520)"}</span>
              <button className="primary" onClick={() => process({ kind: "private" })}>Boeken als privé</button>
            </div>
          )}
          {manual === "ignore" && <IgnoreForm onSubmit={(reason) => process({ kind: "ignore", reason })} />}
        </>
      )}
    </aside>
  );
}

function AccountForm({ t, onSubmit }: { t: BankTransaction; onSubmit: (a: BankAction) => void }) {
  const accounts = useLoad(() => api.accounts(), []);
  const codes = useLoad(() => api.taxCodes(t.booking_date), [t.booking_date]);
  const [account, setAccount] = useState("");
  const [taxCode, setTaxCode] = useState("");
  const [description, setDescription] = useState("");
  const relevant = (codes.data ?? []).filter((c) => (t.amount_cents < 0 ? c.kind !== "sales" : c.kind !== "purchase"));
  return (
    <div className="box">
      <label>Rekening
        <select value={account} onChange={(e) => setAccount(e.target.value)}>
          <option value="">— kies —</option>
          {(accounts.data ?? []).filter((a) => a.active && a.number !== "1100").map((a) => <option key={a.number} value={a.number}>{a.number} {a.name}</option>)}
        </select>
      </label>
      <label>Btw (bedrag is inclusief btw)
        <select value={taxCode} onChange={(e) => setTaxCode(e.target.value)}>
          <option value="">geen btw</option>
          {relevant.map((c) => <option key={c.code} value={c.code}>{c.code} {c.name}</option>)}
        </select>
      </label>
      <label>Omschrijving (optioneel) <input value={description} onChange={(e) => setDescription(e.target.value)} /></label>
      <p className="muted small">Gebruik dit voor kleine kosten zonder factuur (bankkosten, abonnementen). Heb je een factuur? Registreer die bij Inkoopfacturen en koppel daarna de betaling.</p>
      <button className="primary" disabled={!account} onClick={() => onSubmit({ kind: "account", account, tax_code: taxCode || null, description: description || null })}>Boeken</button>
    </div>
  );
}

function InvoicePicker({ t, onSubmit }: { t: BankTransaction; onSubmit: (a: BankAction) => void }) {
  const sales = useLoad(() => (t.amount_cents > 0 ? api.salesInvoices(null) : Promise.resolve([])), [t.id]);
  const purchases = useLoad(() => (t.amount_cents < 0 ? api.purchaseInvoices(null) : Promise.resolve([])), [t.id]);
  const openSales = (sales.data ?? []).filter((i) => i.status === "issued" && !i.is_credit_note && i.open_cents > 0);
  const openPurchases = (purchases.data ?? []).filter((p) => p.state === "open");
  return (
    <div className="box">
      {t.amount_cents > 0 && (openSales.length === 0 ? <span className="muted small">Geen openstaande verkoopfacturen.</span> : openSales.map((i) => (
        <div key={i.id} className="suggestion"><span className="small">{i.invoice_number} {i.contact_name} — open {formatEuro(i.open_cents)}</span>
          <button onClick={() => onSubmit({ kind: "sales_invoice", invoice_id: i.id })}>Koppelen</button></div>
      )))}
      {t.amount_cents < 0 && (openPurchases.length === 0 ? <span className="muted small">Geen openstaande inkoopfacturen.</span> : openPurchases.map((p) => (
        <div key={p.id} className="suggestion"><span className="small">{p.contact_name} {p.supplier_reference ?? ""} — {formatEuro(p.open_cents)}</span>
          <button onClick={() => onSubmit({ kind: "purchase_invoice", invoice_id: p.id })}>Koppelen</button></div>
      )))}
      {t.amount_cents > 0 && (
        <div className="suggestion"><span className="small">Uitbetaling platform</span>
          <span className="row gap">
            <button onClick={() => onSubmit({ kind: "platform_payout", platform: "apple", settle_cents: null })}>Apple</button>
            <button onClick={() => onSubmit({ kind: "platform_payout", platform: "google", settle_cents: null })}>Google</button>
          </span>
        </div>
      )}
    </div>
  );
}

function IgnoreForm({ onSubmit }: { onSubmit: (reason: string) => void }) {
  const [reason, setReason] = useState("");
  return (
    <div className="box">
      <p className="small warning">Negeer alleen transacties die al op een andere manier geboekt zijn; anders klopt het banksaldo niet meer.</p>
      <input placeholder="Reden" value={reason} onChange={(e) => setReason(e.target.value)} />
      <button onClick={() => onSubmit(reason)}>Negeren</button>
    </div>
  );
}
