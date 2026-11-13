import { api } from "../api";
import { useLoad } from "../hooks";
import { formatEuro, formatMinutes, monthNames } from "../money";

type Go = (view: string) => void;

export function DashboardView({ year, go }: { year: number; go: Go }) {
  const d = useLoad(() => api.dashboard(year), [year]);
  const backup = useLoad(() => api.backupStatus(), []);
  if (d.error) return <p className="error">{d.error}</p>;
  if (!d.data) return <p className="muted">Laden…</p>;
  const x = d.data;
  const maxBar = Math.max(1, ...x.revenue_by_month, ...x.expenses_by_month);
  const p = (n: number, one: string, many: string) => (n === 1 ? one : many);
  const todo: [number, string, string][] = [
    [x.bank_unprocessed, p(x.bank_unprocessed, "banktransactie te verwerken", "banktransacties te verwerken"), "bank"],
    [x.purchases_without_document, p(x.purchases_without_document, "inkoopfactuur zonder bewijsstuk", "inkoopfacturen zonder bewijsstuk"), "inkoop"],
    [x.draft_invoices, p(x.draft_invoices, "conceptfactuur", "conceptfacturen"), "verkoop"],
    [x.receivables_overdue, p(x.receivables_overdue, "vervallen verkoopfactuur", "vervallen verkoopfacturen"), "verkoop"],
    [x.unbooked_platform_reports, p(x.unbooked_platform_reports, "platformrapport te boeken", "platformrapporten te boeken"), "platformen"],
    [backup.data && (backup.data.days_since_good ?? 99) >= 7 ? 1 : 0, "back-up nodig: de laatste goede back-up is ouder dan 7 dagen of ontbreekt", "instellingen"],
  ];
  const criterion = 1225 * 60;
  return (
    <section>
      <h2>Overzicht {year}</h2>
      <div className="tiles">
        <Tile label="Omzet" value={formatEuro(x.revenue_cents)} onClick={() => go("proefbalans")} />
        <Tile label="Kosten" value={formatEuro(x.expenses_cents)} onClick={() => go("proefbalans")} />
        <Tile label="Resultaat" value={formatEuro(x.result_cents)} strong onClick={() => go("proefbalans")} />
        <Tile label="Bank" value={formatEuro(x.bank_cents)} onClick={() => go("bank")} />
        <Tile label="Te ontvangen (facturen)" value={formatEuro(x.receivables_open_cents)} onClick={() => go("verkoop")} />
        <Tile label="Te ontvangen (platformen)" value={formatEuro(x.platform_receivable_cents)} onClick={() => go("platformen")} />
        <Tile label="Te betalen (inkoop)" value={formatEuro(x.payables_open_cents)} onClick={() => go("inkoop")} />
        <Tile label={x.vat_position_cents >= 0 ? "Btw te betalen" : "Btw terug te vragen"} value={formatEuro(Math.abs(x.vat_position_cents))} onClick={() => go("btw")} />
      </div>
      <div className="split even">
        <div className="panel">
          <h3>Aandachtspunten</h3>
          {todo.every(([n]) => n === 0)
            ? <p className="ok small">Alles bijgewerkt.</p>
            : <ul className="todo">{todo.filter(([n]) => n > 0).map(([n, label, view]) => (
                <li key={label}><a onClick={() => go(view)}><strong>{n}</strong> {label}</a></li>
              ))}</ul>}
          <h3>Urencriterium</h3>
          <div className="progress"><div style={{ width: `${Math.min(100, (x.hours_minutes / criterion) * 100)}%` }} /></div>
          <p className="small"><a onClick={() => go("uren")}>{formatMinutes(x.hours_minutes)} van 1.225 uur</a></p>
        </div>
        <div className="panel">
          <h3>Omzet en kosten per maand</h3>
          <div className="bars">
            {monthNames.map((m, i) => (
              <div key={m} className="bar-group" title={`${m}: omzet ${formatEuro(x.revenue_by_month[i])}, kosten ${formatEuro(x.expenses_by_month[i])}`}>
                <div className="bar rev" style={{ height: `${(Math.max(0, x.revenue_by_month[i]) / maxBar) * 100}%` }} />
                <div className="bar exp" style={{ height: `${(Math.max(0, x.expenses_by_month[i]) / maxBar) * 100}%` }} />
                <span>{m.slice(0, 3)}</span>
              </div>
            ))}
          </div>
          <p className="small muted"><span className="legend rev" /> omzet <span className="legend exp" /> kosten</p>
        </div>
      </div>
    </section>
  );
}

function Tile({ label, value, strong, onClick }: { label: string; value: string; strong?: boolean; onClick: () => void }) {
  return (
    <button className="tile" onClick={onClick}>
      <span className="muted small">{label}</span>
      <span className={strong ? "tile-value strong" : "tile-value"}>{value}</span>
    </button>
  );
}
