import { useState } from "react";
import { api, type AccountType } from "../api";
import { useLoad } from "../hooks";
import { formatDate, formatEuro } from "../money";

export const typeLabel: Record<AccountType, string> = {
  asset: "Activa",
  liability: "Passiva",
  equity: "Eigen vermogen",
  revenue: "Opbrengsten",
  expense: "Kosten",
};

export function TrialBalance({ year, onOpenAccount }: { year: number; onOpenAccount: (account: string) => void }) {
  const tb = useLoad(() => api.trialBalance(`${year}-12-31`), [year]);
  const [chain, setChain] = useState<string | null>(null);
  const [chainOk, setChainOk] = useState(true);

  const check = () =>
    api.verifyChain().then((c) => {
      if ("Intact" in c) {
        setChainOk(true);
        setChain(`Administratie intact: ${c.Intact} boekingen gecontroleerd.`);
      } else {
        setChainOk(false);
        setChain(`Let op: boeking ${c.Broken.chain_index} is buiten de applicatie gewijzigd.`);
      }
    }).catch((e) => { setChainOk(false); setChain(String(e)); });

  const rows = tb.data ?? [];
  const totalDebit = rows.reduce((s, r) => s + r.debit_cents, 0);
  const totalCredit = rows.reduce((s, r) => s + r.credit_cents, 0);

  return (
    <section>
      <header className="row">
        <h2>Proef- en saldibalans t/m 31-12-{year}</h2>
        <button onClick={check}>Integriteit controleren</button>
      </header>
      <p className="muted small">Cumulatief over alle boekingen tot en met de einddatum.</p>
      {chain && <p className={chainOk ? "ok" : "error"}>{chain}</p>}
      {tb.error && <p className="error">{tb.error}</p>}
      {tb.data && rows.length === 0 && <p className="muted">Nog geen boekingen.</p>}
      {rows.length > 0 && (
        <table className="clickable">
          <thead><tr><th>Rekening</th><th>Naam</th><th className="num">Debet</th><th className="num">Credit</th><th className="num">Saldo</th></tr></thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.account} onClick={() => onOpenAccount(r.account)}>
                <td>{r.account}</td><td className="desc">{r.name}</td>
                <td className="num">{formatEuro(r.debit_cents)}</td>
                <td className="num">{formatEuro(r.credit_cents)}</td>
                <td className="num">{formatEuro(r.balance_cents)}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr><td colSpan={2}>Totaal</td><td className="num">{formatEuro(totalDebit)}</td><td className="num">{formatEuro(totalCredit)}</td><td /></tr>
          </tfoot>
        </table>
      )}
    </section>
  );
}

export function LedgerCardView({ year, account, onAccount }: { year: number; account: string; onAccount: (a: string) => void }) {
  const accounts = useLoad(() => api.accounts(), []);
  const card = useLoad(() => (account ? api.ledgerCard(account, year) : Promise.resolve(null)), [account, year]);

  return (
    <section>
      <header className="row">
        <h2>Grootboekkaart {year}</h2>
        <select value={account} onChange={(e) => onAccount(e.target.value)}>
          <option value="">— kies rekening —</option>
          {accounts.data?.map((a) => <option key={a.number} value={a.number}>{a.number} {a.name}</option>)}
        </select>
      </header>
      {card.error && <p className="error">{card.error}</p>}
      {card.data && (
        <table>
          <thead><tr><th>Datum</th><th>Boeking</th><th>Omschrijving</th><th className="num">Debet</th><th className="num">Credit</th><th className="num">Saldo</th></tr></thead>
          <tbody>
            <tr className="muted"><td colSpan={5}>Beginsaldo</td><td className="num">{formatEuro(card.data.opening_cents)}</td></tr>
            {card.data.rows.map((r, i) => (
              <tr key={i}>
                <td>{formatDate(r.date)}</td><td>{r.number}</td><td className="desc">{r.description}</td>
                <td className="num">{r.debit_cents ? formatEuro(r.debit_cents) : ""}</td>
                <td className="num">{r.credit_cents ? formatEuro(r.credit_cents) : ""}</td>
                <td className="num">{formatEuro(r.balance_cents)}</td>
              </tr>
            ))}
          </tbody>
          <tfoot><tr><td colSpan={5}>Eindsaldo</td><td className="num">{formatEuro(card.data.closing_cents)}</td></tr></tfoot>
        </table>
      )}
    </section>
  );
}

export function ChartOfAccounts() {
  const accounts = useLoad(() => api.accounts(), []);
  return (
    <section>
      <h2>Rekeningschema</h2>
      <p className="muted small">Concept volgens docs/accounting-rules.md; RGS-koppeling volgt na beoordeling door een boekhouder.</p>
      {accounts.error && <p className="error">{accounts.error}</p>}
      {accounts.data && (
        <table>
          <thead><tr><th>Nr</th><th>Naam</th><th>Soort</th><th>RGS</th></tr></thead>
          <tbody>
            {accounts.data.map((a) => (
              <tr key={a.number} className={a.active ? "" : "inactive"}>
                <td>{a.number}</td><td>{a.name}</td><td>{typeLabel[a.account_type]}</td><td>{a.rgs_code ?? "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
