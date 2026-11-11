import { useEffect, useState } from "react";
import { api, type Account, type AppInfo, type ChainStatus, type TrialBalanceRow } from "./api";
import { formatEuro } from "./money";

type View = "proefbalans" | "rekeningschema";

const typeLabel: Record<Account["account_type"], string> = {
  asset: "Activa",
  liability: "Passiva",
  equity: "Eigen vermogen",
  revenue: "Opbrengsten",
  expense: "Kosten",
};

export function App() {
  const [view, setView] = useState<View>("proefbalans");
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.appInfo().then(setInfo).catch((e) => setError(String(e)));
  }, []);

  return (
    <div className="layout">
      <nav>
        <h1>Boekhoud</h1>
        <button className={view === "proefbalans" ? "active" : ""} onClick={() => setView("proefbalans")}>
          Proef- en saldibalans
        </button>
        <button className={view === "rekeningschema" ? "active" : ""} onClick={() => setView("rekeningschema")}>
          Rekeningschema
        </button>
        <footer>{info && <small>v{info.version}<br />{info.data_dir}</small>}</footer>
      </nav>
      <main>
        {error && <p className="error">Fout: {error}</p>}
        {view === "proefbalans" ? <TrialBalance /> : <ChartOfAccounts />}
      </main>
    </div>
  );
}

function TrialBalance() {
  const [rows, setRows] = useState<TrialBalanceRow[] | null>(null);
  const [chain, setChain] = useState<ChainStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.trialBalance().then(setRows).catch((e) => setError(String(e)));
  }, []);

  const check = () => api.verifyChain().then(setChain).catch((e) => setError(String(e)));
  const totalDebit = rows?.reduce((s, r) => s + r.debit_cents, 0) ?? 0;
  const totalCredit = rows?.reduce((s, r) => s + r.credit_cents, 0) ?? 0;

  return (
    <section>
      <header className="row">
        <h2>Proef- en saldibalans</h2>
        <button onClick={check}>Integriteit controleren</button>
      </header>
      {chain && (
        <p className={"Intact" in chain ? "ok" : "error"}>
          {"Intact" in chain
            ? `Administratie intact: ${chain.Intact} boekingen gecontroleerd.`
            : `Let op: boeking ${chain.Broken.chain_index} is gewijzigd buiten de applicatie.`}
        </p>
      )}
      {error && <p className="error">Fout: {error}</p>}
      {rows && rows.length === 0 && <p>Nog geen boekingen.</p>}
      {rows && rows.length > 0 && (
        <table>
          <thead>
            <tr><th>Rekening</th><th>Naam</th><th className="num">Debet</th><th className="num">Credit</th><th className="num">Saldo</th></tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.account}>
                <td>{r.account}</td><td>{r.name}</td>
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

function ChartOfAccounts() {
  const [accounts, setAccounts] = useState<Account[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.accounts().then(setAccounts).catch((e) => setError(String(e)));
  }, []);

  return (
    <section>
      <h2>Rekeningschema</h2>
      {error && <p className="error">Fout: {error}</p>}
      {accounts && (
        <table>
          <thead><tr><th>Nr</th><th>Naam</th><th>Soort</th><th>RGS</th></tr></thead>
          <tbody>
            {accounts.map((a) => (
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
