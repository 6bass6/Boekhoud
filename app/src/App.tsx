import { useState } from "react";
import { api } from "./api";
import { useLoad } from "./hooks";
import { Contacts } from "./views/Contacts";
import { Entries } from "./views/Entries";
import { Platforms } from "./views/Platforms";
import { Purchases } from "./views/Purchases";
import { SalesInvoices } from "./views/SalesInvoices";
import { NewEntry } from "./views/NewEntry";
import { ChartOfAccounts, LedgerCardView, TrialBalance } from "./views/Reports";
import { Periods, Settings } from "./views/Settings";

type View = "verkoop" | "inkoop" | "platformen" | "contacten" | "boekingen" | "nieuw" | "proefbalans" | "grootboek" | "perioden" | "rekeningschema" | "instellingen";

const nav: { view: View; label: string; section?: string }[] = [
  { view: "verkoop", label: "Verkoopfacturen", section: "Administratie" },
  { view: "inkoop", label: "Inkoopfacturen" },
  { view: "platformen", label: "Platformen" },
  { view: "contacten", label: "Contacten" },
  { view: "boekingen", label: "Boekingen", section: "Boekhouding" },
  { view: "nieuw", label: "Nieuwe boeking" },
  { view: "proefbalans", label: "Proef- en saldibalans", section: "Overzichten" },
  { view: "grootboek", label: "Grootboekkaart" },
  { view: "perioden", label: "Perioden", section: "Beheer" },
  { view: "rekeningschema", label: "Rekeningschema" },
  { view: "instellingen", label: "Instellingen" },
];

export function App() {
  const [view, setView] = useState<View>("verkoop");
  const [year, setYear] = useState(new Date().getFullYear());
  const [account, setAccount] = useState("");
  const info = useLoad(() => api.appInfo(), []);

  const openAccount = (a: string) => { setAccount(a); setView("grootboek"); };

  return (
    <div className="layout">
      <nav>
        <h1>Boekhoud</h1>
        <label className="year">Boekjaar
          <select value={year} onChange={(e) => setYear(Number(e.target.value))}>
            {Array.from({ length: 8 }, (_, i) => new Date().getFullYear() + 1 - i).map((y) => <option key={y}>{y}</option>)}
          </select>
        </label>
        {nav.map((n) => (
          <div key={n.view} className="nav-item">
            {n.section && <div className="nav-section">{n.section}</div>}
            <button className={view === n.view ? "active" : ""} onClick={() => setView(n.view)}>{n.label}</button>
          </div>
        ))}
        <footer>{info.data && <small>v{info.data.version}<br />{info.data.data_dir}</small>}</footer>
      </nav>
      <main>
        {info.error && <p className="error">{info.error}</p>}
        {view === "verkoop" && <SalesInvoices year={year} />}
        {view === "inkoop" && <Purchases year={year} />}
        {view === "platformen" && <Platforms year={year} />}
        {view === "contacten" && <Contacts />}
        {view === "boekingen" && <Entries year={year} />}
        {view === "nieuw" && <NewEntry onPosted={() => undefined} />}
        {view === "proefbalans" && <TrialBalance year={year} onOpenAccount={openAccount} />}
        {view === "grootboek" && <LedgerCardView year={year} account={account} onAccount={setAccount} />}
        {view === "perioden" && <Periods year={year} />}
        {view === "rekeningschema" && <ChartOfAccounts />}
        {view === "instellingen" && <Settings />}
      </main>
    </div>
  );
}
