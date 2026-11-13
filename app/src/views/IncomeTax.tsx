import { useState, type ReactNode } from "react";
import { api } from "../api";
import { useLoad } from "../hooks";
import { formatEuro, formatMinutes, parseCents } from "../money";

/** Micro-units -> Dutch percentage text without floating point: 357500 -> "35,75%". */
function formatRate(micro: number): string {
  const abs = Math.abs(micro);
  const frac = String(abs % 10000).padStart(4, "0").replace(/0+$/, "");
  return `${micro < 0 ? "-" : ""}${Math.floor(abs / 10000)}${frac ? "," + frac : ""}%`;
}

export function IncomeTax({ year }: { year: number }) {
  const [salary, setSalary] = useState("");
  const [withheld, setWithheld] = useState("");
  const [starter, setStarter] = useState(false);
  const valid = (t: string) => t.trim() === "" || parseCents(t) !== null;
  // Invalid input is not sent; the field shows an error instead.
  const view = useLoad(
    () => api.incomeTaxEstimate(year, valid(salary) ? salary : "", valid(withheld) ? withheld : "", starter),
    [year, salary, withheld, starter],
  );
  const v = view.data;
  const e = v?.estimate;
  const p = v?.params;
  const hoursMet = v ? v.hours_minutes >= v.criterion_hours * 60 : false;

  const row = (label: string, note: ReactNode, cents: number, total = false) => (
    <tr>
      <td>{total ? <strong>{label}</strong> : label}</td>
      <td className="small muted">{note}</td>
      <td className="num">{total ? <strong>{formatEuro(cents)}</strong> : formatEuro(cents)}</td>
    </tr>
  );

  return (
    <section>
      <h2>IB-prognose {year}</h2>
      <p className="warning"><strong>Let op:</strong> winst in de boekhouding ≠ fiscale winst ≠ belastbaar inkomen ≠ definitieve aanslag. Dit is een schatting.</p>
      <div className="split">
        <div>
          {view.error && <p className="error">{view.error}</p>}
          {v && v.parameter_year !== v.year && (
            <div className="box warn-box">Voor {v.year} zijn nog geen definitieve belastingparameters ingevoerd. Deze prognose rekent met de parameters van <strong>{v.parameter_year}</strong>.</div>
          )}
          {v && e && p && (
            <table>
              <thead><tr><th>Stap</th><th>Toelichting</th><th className="num">Bedrag</th></tr></thead>
              <tbody>
                {row("1. Winst uit de boekhouding", `resultaat ${v.year} (opbrengsten − kosten)`, e.profit_cents)}
                {row("2. Zelfstandigenaftrek", hoursMet
                  ? `urencriterium gehaald (${formatMinutes(v.hours_minutes)} van ${v.criterion_hours.toLocaleString("nl-NL")} uur)`
                  : `urencriterium niet gehaald (${formatMinutes(v.hours_minutes)} van ${v.criterion_hours.toLocaleString("nl-NL")} uur)`, -e.zelfstandigenaftrek_cents)}
                {row("Startersaftrek", starter ? (hoursMet ? "starter aangevinkt" : "alleen met urencriterium") : "niet aangevinkt", -e.startersaftrek_cents)}
                {row("3. Mkb-winstvrijstelling", `${formatRate(p.mkb_rate_micro)} van winst na ondernemersaftrek`, -e.mkb_vrijstelling_cents)}
                {row("Belastbare winst uit onderneming", "", e.taxable_profit_cents, true)}
                {row("4. Loon uit dienstbetrekking", "eigen invoer", e.salary_cents)}
                {row("Inkomen box 1", "", e.box1_income_cents, true)}
                {e.brackets.map((b, i) => (
                  <tr key={i}>
                    <td>Schijf {i + 1}</td>
                    <td className="small muted">{formatRate(b.rate_micro)} over {formatEuro(b.income_cents)}{b.upto_cents !== null ? ` (t/m ${formatEuro(b.upto_cents)})` : ""}</td>
                    <td className="num">{formatEuro(b.tax_cents)}</td>
                  </tr>
                ))}
                {row("5. Aftrekbeperking", `aftrek in hoogste schijf (${formatEuro(e.rate_cap_base_cents)}) levert max. ${formatRate(p.deduction_rate_cap_micro)} op`, e.rate_cap_cents)}
                {row("Inkomstenbelasting vóór heffingskortingen", "", e.tax_cents, true)}
                {row("6. Heffingskortingen", `algemene heffingskorting ${formatEuro(e.algemene_heffingskorting_cents)} + arbeidskorting ${formatEuro(e.arbeidskorting_cents)}, samen maximaal de belasting`, -e.credits_applied_cents)}
                {row("Inkomstenbelasting na heffingskortingen", "", e.tax_after_credits_cents, true)}
                {row("7. Bijdrage Zvw", `${formatRate(p.zvw_rate_micro)} over ${formatEuro(e.zvw_base_cents)} (max. ${formatEuro(p.zvw_max_income_cents)} min loon)`, e.zvw_cents)}
                {row("Totaal verschuldigd", "", e.total_cents, true)}
                {row("8. Al ingehouden loonheffing", "eigen invoer", -e.withheld_cents)}
                {row(e.to_pay_cents >= 0 ? "Nog te betalen" : "Terug te ontvangen", "", Math.abs(e.to_pay_cents), true)}
              </tbody>
            </table>
          )}
        </div>
        <div className="panel">
          <h3>Gegevens</h3>
          <div className="form-grid">
            <label>Loon uit dienstbetrekking {year} (bruto, fiscaal)
              <input value={salary} onChange={(ev) => setSalary(ev.target.value)} placeholder="0,00" />
              {!valid(salary) && <span className="error">Ongeldig bedrag</span>}
            </label>
            <label>Ingehouden loonheffing {year}
              <input value={withheld} onChange={(ev) => setWithheld(ev.target.value)} placeholder="0,00" />
              {!valid(withheld) && <span className="error">Ongeldig bedrag</span>}
            </label>
            <label><span><input type="checkbox" checked={starter} onChange={(ev) => setStarter(ev.target.checked)} /> Recht op startersaftrek</span></label>
          </div>
          <p className="small muted">Deze gegevens worden niet opgeslagen. Loon en loonheffing staan op je loonstrook of jaaropgaaf; schat ze voor het hele jaar.
            Startersaftrek: in een of meer van de vorige 5 jaar geen ondernemer en hooguit 2 keer zelfstandigenaftrek gehad.</p>
          {e && (
            <div className="box">
              <div className="row"><span>{e.to_pay_cents >= 0 ? "Nog te betalen" : "Terug te ontvangen"}</span><strong>{formatEuro(Math.abs(e.to_pay_cents))}</strong></div>
              {e.monthly_reserve_cents !== null && v && (
                <div className="row"><span>Reserveren per maand ({v.remaining_months} {v.remaining_months === 1 ? "maand" : "maanden"} te gaan)</span><strong>{formatEuro(e.monthly_reserve_cents)}</strong></div>
              )}
            </div>
          )}
          <p className="small muted">Niet meegenomen: box 2 en 3, partner en toeslagen, AOW-leeftijd, eigen woning, fiscale correcties op de winst (zoals beperkt aftrekbare kosten), verliesverrekening met andere jaren en voorlopige aanslagen die je al betaalt.</p>
          {v && (
            <div className="small muted">Bronnen (parameters {v.parameter_year}, belastingdienst.nl):
              <ul>{v.sources.map((s) => <li key={s} style={{ wordBreak: "break-all" }}>{s}</li>)}</ul>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
