import { useState } from "react";
import { api, type AccountType, type ReportKind, type ReportLine } from "../api";
import { useLoad } from "../hooks";
import { formatDate, formatEuro } from "../money";

export const typeLabel: Record<AccountType, string> = {
  asset: "Activa",
  liability: "Passiva",
  equity: "Eigen vermogen",
  revenue: "Opbrengsten",
  expense: "Kosten",
};

/** Button that writes the report as CSV to the data folder and shows where it went. */
function ExportButton({ report, until }: { report: ReportKind; until: string }) {
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const run = () =>
    api.exportReportCsv(report, until)
      .then((path) => setMsg({ ok: true, text: `Opgeslagen als ${path}` }))
      .catch((e) => setMsg({ ok: false, text: String(e) }));
  return (
    <>
      <button onClick={run}>Exporteren (CSV)</button>
      {msg && <p className={msg.ok ? "ok small" : "error"}>{msg.text}</p>}
    </>
  );
}

function ReportRows({ title, lines, totalLabel, total, extra = [] }: {
  title: string; lines: ReportLine[]; totalLabel: string; total: number; extra?: { name: string; amount_cents: number }[];
}) {
  return (
    <>
      <tr><th colSpan={3}>{title}</th></tr>
      {lines.map((l) => (
        <tr key={l.account}><td>{l.account}</td><td className="desc">{l.name}</td><td className="num">{formatEuro(l.amount_cents)}</td></tr>
      ))}
      {extra.map((l) => (
        <tr key={l.name}><td /><td className="desc"><em>{l.name}</em></td><td className="num">{formatEuro(l.amount_cents)}</td></tr>
      ))}
      <tr className="total"><td /><td><strong>{totalLabel}</strong></td><td className="num"><strong>{formatEuro(total)}</strong></td></tr>
    </>
  );
}

export function ProfitAndLossView({ year }: { year: number }) {
  const [until, setUntil] = useState(`${year}-12-31`);
  const pl = useLoad(() => api.profitAndLoss(until), [until]);
  const p = pl.data;
  return (
    <section>
      <header className="row">
        <h2>Winst & verlies {year}</h2>
        <label>t/m <input type="date" min={`${year}-01-01`} max={`${year}-12-31`} value={until} onChange={(e) => e.target.value && setUntil(e.target.value)} /></label>
        <ExportButton report="winst-verlies" until={until} />
      </header>
      <p className="muted small">Alleen boekingen van 1 januari {year} tot en met de gekozen datum.</p>
      {pl.error && <p className="error">{pl.error}</p>}
      {p && (
        <table>
          <tbody>
            <ReportRows title="Opbrengsten" lines={p.revenue} totalLabel="Totaal opbrengsten" total={p.revenue_cents} />
            <ReportRows title="Kosten" lines={p.expenses} totalLabel="Totaal kosten" total={p.expenses_cents} />
          </tbody>
          <tfoot><tr><td colSpan={2}>Resultaat</td><td className="num">{formatEuro(p.result_cents)}</td></tr></tfoot>
        </table>
      )}
    </section>
  );
}

export function BalanceSheetView({ year }: { year: number }) {
  const [until, setUntil] = useState(`${year}-12-31`);
  const bs = useLoad(() => api.balanceSheet(until), [until]);
  const b = bs.data;
  return (
    <section>
      <header className="row">
        <h2>Balans per {formatDate(until)}</h2>
        <label>Datum <input type="date" value={until} onChange={(e) => e.target.value && setUntil(e.target.value)} /></label>
        <ExportButton report="balans" until={until} />
      </header>
      <p className="muted small">
        Er is nog geen jaarafsluiting: de resultaten van voorgaande jaren en van het lopende jaar zijn berekend, niet geboekt.
      </p>
      {bs.error && <p className="error">{bs.error}</p>}
      {b && (
        <table>
          <tbody>
            <ReportRows title="Activa" lines={b.assets} totalLabel="Totaal activa" total={b.assets_cents} />
            <ReportRows title="Vreemd vermogen" lines={b.liabilities} totalLabel="Totaal vreemd vermogen" total={b.liabilities_cents} />
            <ReportRows title="Eigen vermogen" lines={b.equity} totalLabel="Totaal eigen vermogen" total={b.equity_cents} extra={[
              { name: "Resultaat voorgaande boekjaren (berekend)", amount_cents: b.prior_years_result_cents },
              { name: "Resultaat lopend boekjaar (berekend)", amount_cents: b.current_year_result_cents },
            ]} />
          </tbody>
          <tfoot><tr><td colSpan={2}>Totaal passiva</td><td className="num">{formatEuro(b.liabilities_cents + b.equity_cents)}</td></tr></tfoot>
        </table>
      )}
    </section>
  );
}

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
        <ExportButton report="proefbalans" until={`${year}-12-31`} />
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
