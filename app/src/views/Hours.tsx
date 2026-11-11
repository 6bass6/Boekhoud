import { useEffect, useState } from "react";
import { api, type TimeEntry } from "../api";
import { useLoad } from "../hooks";
import { formatDate, formatMinutes, monthNames, todayIso } from "../money";

export const activities = ["Ontwikkeling", "Lezingen", "Acquisitie", "Marketing", "Administratie", "Opleiding", "Overig"];

export function Hours({ year }: { year: number }) {
  const entries = useLoad(() => api.timeEntries(year), [year]);
  const summary = useLoad(() => api.hoursSummary(year), [year]);
  const timer = useLoad(() => api.runningTimer(), []);
  const [form, setForm] = useState({ id: null as string | null, entry_date: todayIso(), hours: "", activity: activities[0], project: "", description: "" });
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(Date.now());

  useEffect(() => { const t = setInterval(() => setNow(Date.now()), 15000); return () => clearInterval(t); }, []);
  const reload = () => { entries.reload(); summary.reload(); timer.reload(); };
  const run = (p: Promise<unknown>) => { setError(null); p.then(reload).catch((e) => setError(String(e))); };
  const save = () => run(api.saveTimeEntry({ ...form, project: form.project || null, description: form.description || null }).then(() =>
    setForm({ ...form, id: null, hours: "", description: "" })));
  const edit = (e: TimeEntry) => setForm({ id: e.id, entry_date: e.entry_date, hours: formatMinutes(e.minutes ?? 0), activity: e.activity, project: e.project ?? "", description: e.description ?? "" });

  const s = summary.data;
  const pct = s ? Math.min(100, (s.total_minutes / (s.criterion_hours * 60)) * 100) : 0;
  const running = timer.data;
  const runningMinutes = running?.started_at ? Math.max(0, Math.floor((now - Date.parse(running.started_at)) / 60000)) : 0;

  return (
    <section>
      <h2>Urenregistratie {year}</h2>
      {s && (
        <div className="panel">
          <div className="row"><strong>{formatMinutes(s.total_minutes)} van {s.criterion_hours.toLocaleString("nl-NL")} uur</strong>
            <span className="small muted">{s.needed_per_week_minutes > 0 ? `nog ${formatMinutes(s.needed_per_week_minutes)} per week nodig` : "urencriterium gehaald"}</span></div>
          <div className="progress"><div style={{ width: `${pct}%` }} /></div>
          <div className="months">{s.by_month.map((m, i) => <span key={i} title={monthNames[i]}>{monthNames[i].slice(0, 3)}<br /><strong>{Math.round(m / 60)}</strong></span>)}</div>
          <p className="small muted">Alle uren voor je onderneming tellen mee, ook administratie en acquisitie. Bij een baan in loondienst gelden extra voorwaarden; zie docs/accounting-rules.md.</p>
        </div>
      )}
      <div className="split even">
        <div className="panel">
          <h3>Timer</h3>
          {running
            ? <div className="row"><span>{running.activity} — <strong>{formatMinutes(runningMinutes)}</strong></span><button className="primary" onClick={() => run(api.stopTimer())}>Stoppen</button></div>
            : <div className="row gap">
                <select value={form.activity} onChange={(e) => setForm({ ...form, activity: e.target.value })}>{activities.map((a) => <option key={a}>{a}</option>)}</select>
                <button className="primary" onClick={() => run(api.startTimer(form.activity, form.description || null))}>Start</button>
              </div>}
          <h3>{form.id ? "Uren wijzigen" : "Uren invoeren"}</h3>
          <div className="form-grid two">
            <label>Datum <input type="date" value={form.entry_date} onChange={(e) => setForm({ ...form, entry_date: e.target.value })} /></label>
            <label>Duur (1,5 of 1:30) <input value={form.hours} onChange={(e) => setForm({ ...form, hours: e.target.value })} /></label>
            <label>Activiteit <select value={form.activity} onChange={(e) => setForm({ ...form, activity: e.target.value })}>{activities.map((a) => <option key={a}>{a}</option>)}</select></label>
            <label>Project / app <input value={form.project} onChange={(e) => setForm({ ...form, project: e.target.value })} /></label>
            <label className="wide">Omschrijving <input value={form.description} onChange={(e) => setForm({ ...form, description: e.target.value })} /></label>
          </div>
          {error && <p className="error">{error}</p>}
          <span className="row gap"><button className="primary" onClick={save}>Opslaan</button>{form.id && <button onClick={() => setForm({ ...form, id: null, hours: "" })}>Annuleren</button>}</span>
        </div>
        <div>
          <table className="clickable">
            <thead><tr><th>Datum</th><th>Activiteit</th><th>Omschrijving</th><th className="num">Duur</th><th /></tr></thead>
            <tbody>{(entries.data ?? []).filter((e) => e.minutes !== null).map((e) => (
              <tr key={e.id} onClick={() => edit(e)}>
                <td>{formatDate(e.entry_date)}</td><td>{e.activity}</td><td className="desc">{[e.project, e.description].filter(Boolean).join(" — ")}</td>
                <td className="num">{formatMinutes(e.minutes ?? 0)}</td>
                <td><button onClick={(ev) => { ev.stopPropagation(); run(api.deleteTimeEntry(e.id)); }}>×</button></td>
              </tr>
            ))}</tbody>
          </table>
        </div>
      </div>
    </section>
  );
}
