import { useState } from "react";
import { api, type DocumentInfo, type PurchaseDetail, type PurchaseForm, type PurchaseLineForm, type PurchaseSuggestion, type PurchaseSummary } from "../api";
import { DocumentTabs, DocumentUpload } from "../components/DocumentViewer";
import { useLoad } from "../hooks";
import { centsToInput, formatDate, formatEuro, parseCents, todayIso, vatFor } from "../money";
import { ContactForm, emptyContact } from "./Contacts";
import type { Contact } from "../api";

const stateLabel: Record<PurchaseSummary["state"], string> = {
  open: "Open", paid: "Betaald", paid_privately: "Privé betaald", reversed: "Tegengeboekt", corrected: "Gecorrigeerd",
};
const inactive = (state: PurchaseSummary["state"]) => state === "reversed" || state === "corrected";

export function Purchases({ year }: { year: number }) {
  const list = useLoad(() => api.purchaseInvoices(year), [year]);
  const [selected, setSelected] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [correcting, setCorrecting] = useState<PurchaseDetail | null>(null);

  if (creating || correcting) {
    return <PurchaseEditor correcting={correcting} onDone={(id) => {
      setCreating(false); setCorrecting(null); list.reload(); if (id) setSelected(id);
    }} />;
  }
  const missingDocs = (list.data ?? []).filter((p) => !p.has_document && !inactive(p.state)).length;
  return (
    <section className="split">
      <div>
        <header className="row">
          <h2>Inkoopfacturen {year}</h2>
          <button className="primary" onClick={() => setCreating(true)}>+ Inkoopfactuur</button>
        </header>
        {missingDocs > 0 && <p className="warning small">{missingDocs} inkoopfactu{missingDocs === 1 ? "ur" : "ren"} zonder bewijsstuk.</p>}
        {list.error && <p className="error">{list.error}</p>}
        {list.data?.length === 0 && <p className="muted">Nog geen inkoopfacturen in {year}.</p>}
        {list.data && list.data.length > 0 && (
          <table className="clickable">
            <thead><tr><th>Datum</th><th>Leverancier</th><th>Omschrijving</th><th>Status</th><th className="num">Totaal</th><th>Bon</th></tr></thead>
            <tbody>
              {list.data.map((p) => (
                <tr key={p.id} className={(selected === p.id ? "selected " : "") + (inactive(p.state) ? "inactive" : "")} onClick={() => setSelected(p.id)}>
                  <td>{formatDate(p.invoice_date)}</td>
                  <td>{p.contact_name}</td>
                  <td className="desc">{p.description}</td>
                  <td><span className="tag">{stateLabel[p.state]}</span></td>
                  <td className="num">{formatEuro(p.total_cents)}</td>
                  <td>{p.has_document ? "✓" : <span className="warning">ontbreekt</span>}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      {selected && <PurchasePanel id={selected} onChanged={list.reload} onSelect={setSelected} onCorrect={setCorrecting} />}
    </section>
  );
}

function PurchasePanel({ id, onChanged, onSelect, onCorrect }: {
  id: string; onChanged: () => void; onSelect: (id: string) => void; onCorrect: (d: PurchaseDetail) => void;
}) {
  const detail = useLoad(() => api.purchaseInvoice(id), [id]);
  const [error, setError] = useState<string | null>(null);
  const [payDate, setPayDate] = useState(todayIso());
  const [editing, setEditing] = useState(false);
  const run = (p: Promise<unknown>) => { setError(null); p.then(() => { detail.reload(); onChanged(); }).catch((e) => setError(String(e))); };
  const d = detail.data;
  if (detail.error) return <aside className="panel"><p className="error">{detail.error}</p></aside>;
  if (!d) return <aside className="panel" />;
  return (
    <aside className="panel">
      <h3>{d.summary.contact_name}</h3>
      {d.summary.corrected_by_id && (
        <p className="warning small">Deze factuur is gecorrigeerd en telt niet meer mee.{" "}
          <button className="link" onClick={() => onSelect(d.summary.corrected_by_id!)}>Nieuwe versie bekijken</button></p>
      )}
      {d.summary.corrects_id && (
        <p className="muted small">Dit is een correctie.{" "}
          <button className="link" onClick={() => onSelect(d.summary.corrects_id!)}>Oude versie bekijken</button></p>
      )}
      {editing
        ? <DetailsForm detail={d} onCancel={() => setEditing(false)}
            onSaved={() => { setEditing(false); detail.reload(); onChanged(); }} />
        : <p>{d.summary.description}<br /><span className="muted small">{formatDate(d.summary.invoice_date)}{d.summary.supplier_reference && ` · factuur ${d.summary.supplier_reference}`}{d.summary.due_date && ` · vervalt ${formatDate(d.summary.due_date)}`}</span></p>}
      {!editing && !inactive(d.summary.state) && (
        <div className="row gap">
          <button onClick={() => setEditing(true)} title="Omschrijving, factuurnummer en vervaldatum; de boeking blijft gelijk">Gegevens wijzigen</button>
          <button onClick={() => onCorrect(d)} title="Bedragen, datum, leverancier of rekening aanpassen: de oude boeking wordt tegengeboekt en de nieuwe geboekt">Corrigeren…</button>
        </div>
      )}
      <table>
        <tbody>
          {d.lines.map((l, i) => <tr key={i}><td>{l.account} {l.description}</td><td>{l.tax_code}</td><td className="num">{formatEuro(l.net_cents)}</td><td className="num">{formatEuro(l.vat_cents)}</td></tr>)}
        </tbody>
        <tfoot><tr><td colSpan={3}>Totaal te betalen</td><td className="num">{formatEuro(d.summary.total_cents)}</td></tr></tfoot>
      </table>
      {error && <p className="error">{error}</p>}
      {d.summary.state === "open" && (
        <div className="box">
          <strong className="small">Betaling registreren ({formatEuro(d.summary.open_cents)} open)</strong>
          <div className="row gap">
            <input type="date" value={payDate} onChange={(e) => setPayDate(e.target.value)} />
            <button onClick={() => run(api.registerPurchasePayment(id, payDate, centsToInput(d.summary.open_cents)))}>Volledig betaald</button>
          </div>
        </div>
      )}
      {d.documents.length === 0 && !inactive(d.summary.state) && <div className="box"><span className="warning small">Geen bewijsstuk gekoppeld.</span></div>}
      {!inactive(d.summary.state) && (
        <DocumentUpload type="purchase_invoice" multiple label={d.documents.length ? "Bestanden toevoegen…" : "Bewijsstukken toevoegen…"}
          onUploaded={(docs) => run(api.attachPurchaseDocuments(id, docs.map((x) => x.id)))} />
      )}
      <DocumentTabs key={id} docs={d.documents} height={420} />
    </aside>
  );
}

/** Quick edit of what is not booked: description, supplier's invoice number, due date. */
function DetailsForm({ detail, onCancel, onSaved }: { detail: PurchaseDetail; onCancel: () => void; onSaved: () => void }) {
  const [description, setDescription] = useState(detail.summary.description);
  const [reference, setReference] = useState(detail.summary.supplier_reference ?? "");
  const [due, setDue] = useState(detail.summary.due_date ?? "");
  const [error, setError] = useState<string | null>(null);
  const save = () => api.updatePurchaseDetails(detail.summary.id, description, reference || null, due || null)
    .then(onSaved).catch((e) => setError(String(e)));
  return (
    <div className="box">
      <div className="form-grid">
        <label className="wide">Omschrijving <input value={description} onChange={(e) => setDescription(e.target.value)} /></label>
        <label>Factuurnummer leverancier <input value={reference} onChange={(e) => setReference(e.target.value)} /></label>
        <label>Vervaldatum <input type="date" value={due} onChange={(e) => setDue(e.target.value)} /></label>
      </div>
      <p className="muted small">Bedragen, datum of leverancier aanpassen? Gebruik dan Corrigeren.</p>
      {error && <p className="error">{error}</p>}
      <div className="row gap">
        <button className="primary" onClick={save}>Opslaan</button>
        <button onClick={onCancel}>Annuleren</button>
      </div>
    </div>
  );
}

const emptyLine = (code: string): PurchaseLineForm => ({ description: null, account: "", net: "", tax_code: code, vat: "" });

function PurchaseEditor({ onDone, correcting }: { onDone: (id: string | null) => void; correcting?: PurchaseDetail | null }) {
  const contacts = useLoad(() => api.contacts(), []);
  const accounts = useLoad(() => api.purchaseAccounts(), []);
  const [form, setForm] = useState<PurchaseForm>(() => correcting ? {
    contact_id: correcting.summary.contact_id,
    supplier_reference: correcting.summary.supplier_reference,
    invoice_date: correcting.summary.invoice_date,
    due_date: correcting.summary.due_date,
    description: correcting.summary.description,
    paid_privately: correcting.summary.state === "paid_privately",
    document_ids: [],
    lines: correcting.lines.map((l) => ({
      description: l.description, account: l.account, net: centsToInput(l.net_cents), tax_code: l.tax_code, vat: centsToInput(l.vat_cents),
    })),
  } : {
    contact_id: "", supplier_reference: null, invoice_date: todayIso(), due_date: null, description: "",
    paid_privately: false, document_ids: [], lines: [emptyLine("I21")],
  });
  const taxCodes = useLoad(() => api.taxCodes(form.invoice_date || todayIso()), [form.invoice_date]);
  const regime = useLoad(() => api.vatRegime(form.invoice_date || todayIso()), [form.invoice_date]);
  const [docs, setDocs] = useState<DocumentInfo[]>(correcting?.documents ?? []);
  const [newContact, setNewContact] = useState<Partial<Contact> | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reading, setReading] = useState(false);
  const [readResult, setReadResult] = useState<{ file: string; source: string; filled: string[]; supplier: PurchaseSuggestion | null } | null>(null);

  const purchaseCodes = (taxCodes.data ?? []).filter((t) => t.kind !== "sales");
  const setLine = (i: number, patch: Partial<PurchaseLineForm>) =>
    setForm((f) => ({ ...f, lines: f.lines.map((l, j) => (j === i ? { ...l, ...patch } : l)) }));
  const suggestVat = (net: string, code: string) => {
    const cents = parseCents(net);
    const tc = purchaseCodes.find((t) => t.code === code);
    return cents !== null && tc?.rate_bp && tc.kind === "purchase" ? centsToInput(vatFor(cents, tc.rate_bp)) : "";
  };
  const total = form.lines.reduce((s, l) => {
    const tc = purchaseCodes.find((t) => t.code === l.tax_code);
    const vat = tc && !tc.rubric?.startsWith("4") ? parseCents(l.vat) ?? 0 : 0;
    return s + (parseCents(l.net) ?? 0) + vat;
  }, 0);
  const pickContact = (id: string) => {
    const c = contacts.data?.find((x) => x.id === id);
    setForm((f) => ({
      ...f,
      contact_id: id,
      lines: f.lines.map((l, i) => i === 0 && !l.account ? { ...l, account: c?.default_account ?? "", tax_code: c?.default_tax_code ?? l.tax_code } : l),
    }));
  };
  /** Fill empty fields from the document; never overwrite what was typed, never book anything. */
  const readDocument = (doc: DocumentInfo) => {
    setReading(true);
    setError(null);
    api.extractPurchase(doc.id).then((s) => {
      const filled: string[] = [];
      const contact = s.contact_id ? contacts.data?.find((c) => c.id === s.contact_id) : undefined;
      const f = form;
      const next = { ...f, lines: [...f.lines] };
      {
        if (!f.contact_id && contact) { next.contact_id = contact.id; filled.push("leverancier"); }
        if (!f.supplier_reference && s.supplier_reference) { next.supplier_reference = s.supplier_reference; filled.push("factuurnummer"); }
        if (s.invoice_date && f.invoice_date === todayIso() && s.invoice_date !== f.invoice_date) { next.invoice_date = s.invoice_date; filled.push("factuurdatum"); }
        if (!f.due_date && s.due_date) { next.due_date = s.due_date; filled.push("vervaldatum"); }
        const first = next.lines[0];
        if (next.lines.length === 1 && first && !first.net.trim() && s.net_cents !== null) {
          next.lines[0] = {
            ...first,
            net: centsToInput(s.net_cents),
            vat: s.vat_cents !== null ? centsToInput(s.vat_cents) : first.vat,
            tax_code: s.tax_code ?? contact?.default_tax_code ?? first.tax_code,
            account: first.account || s.account || contact?.default_account || "",
          };
          filled.push("bedragen");
        }
      }
      setForm(next);
      const unknownSupplier = !s.contact_id && (s.supplier_name || s.supplier_vat || s.supplier_kvk) ? s : null;
      setReadResult({ file: doc.original_filename, source: s.source, filled, supplier: unknownSupplier });
    }).catch((e) => setError(String(e))).finally(() => setReading(false));
  };

  const save = () => {
    const invoice = { ...form, document_ids: docs.map((x) => x.id) };
    (correcting ? api.correctPurchase(correcting.summary.id, invoice) : api.registerPurchase(invoice))
      .then((s) => onDone(s.id)).catch((e) => setError(String(e)));
  };

  return (
    <section className="split wide-right">
      <div>
        <header className="row">
          <h2>{correcting ? "Inkoopfactuur corrigeren" : "Inkoopfactuur registreren"}</h2>
          <button onClick={() => onDone(correcting?.summary.id ?? null)}>Terug</button>
        </header>
        {correcting && <p className="muted small">Pas aan wat niet klopt. Bij opslaan wordt de oude boeking tegengeboekt (op de oude factuurdatum) en de
          gecorrigeerde factuur opnieuw geboekt. De oude versie blijft zichtbaar als "Gecorrigeerd".</p>}
        {reading && <p className="muted small">Gegevens uit het bestand lezen…</p>}
        {readResult && (
          <div className="box small">
            {readResult.filled.length > 0
              ? <>Ingevuld uit <strong>{readResult.file}</strong>: {readResult.filled.join(", ")}.{" "}
                  {readResult.source === "ubl" ? "Uit de e-factuur gelezen." : <strong>Automatisch herkend: controleer alles voordat je boekt.</strong>}</>
              : <>Uit <strong>{readResult.file}</strong> kon niets nieuws worden ingevuld; velden die al gevuld waren zijn niet aangepast.</>}
            {readResult.supplier && (
              <div className="row gap" style={{ marginTop: 6 }}>
                <span>Leverancier niet gevonden: {[readResult.supplier.supplier_name, readResult.supplier.supplier_vat, readResult.supplier.supplier_kvk && `KvK ${readResult.supplier.supplier_kvk}`].filter(Boolean).join(" · ")}</span>
                <button onClick={() => setNewContact({
                  name: readResult.supplier?.supplier_name ?? "",
                  vat_number: readResult.supplier?.supplier_vat ?? null,
                  kvk_number: readResult.supplier?.supplier_kvk ?? null,
                  country: readResult.supplier?.supplier_vat?.slice(0, 2) || "NL",
                })}>Toevoegen als leverancier…</button>
              </div>
            )}
          </div>
        )}
        <div className="form-grid">
          <label className="wide">Leverancier
            <span className="row gap">
              <select value={form.contact_id} onChange={(e) => pickContact(e.target.value)} style={{ flex: 1 }}>
                <option value="">— kies leverancier —</option>
                {(contacts.data ?? []).filter((c) => c.is_supplier || c.id === form.contact_id).map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
              </select>
              <button onClick={() => setNewContact({})}>Nieuw…</button>
            </span>
          </label>
          <label>Factuurdatum <input type="date" value={form.invoice_date} onChange={(e) => setForm({ ...form, invoice_date: e.target.value })} /></label>
          <label>Factuurnummer leverancier <input value={form.supplier_reference ?? ""} onChange={(e) => setForm({ ...form, supplier_reference: e.target.value || null })} /></label>
          <label>Vervaldatum <input type="date" value={form.due_date ?? ""} onChange={(e) => setForm({ ...form, due_date: e.target.value || null })} /></label>
          <label className="wide">Omschrijving <input value={form.description} onChange={(e) => setForm({ ...form, description: e.target.value })} /></label>
        </div>
        {newContact && (
          <ContactForm initial={{ ...emptyContact(), is_customer: false, is_supplier: true, ...newContact }} onCancel={() => setNewContact(null)}
            onSaved={(id) => { setNewContact(null); setReadResult((r) => r && { ...r, supplier: null }); contacts.reload(); pickContact(id); }} />
        )}
        {regime.data === "kor" && <p className="muted small">Onder de KOR is btw niet aftrekbaar: boek het bedrag inclusief btw met code GEEN.</p>}
        <table className="entry-lines">
          <thead><tr><th>Kostenrekening</th><th>Btw-code</th><th className="num">Bedrag excl.</th><th className="num">Btw</th><th /></tr></thead>
          <tbody>
            {form.lines.map((l, i) => (
              <tr key={i}>
                <td>
                  <select value={l.account} onChange={(e) => setLine(i, { account: e.target.value })}>
                    <option value="">— kies —</option>
                    {(accounts.data ?? []).filter((a) => a.account_type === "expense").map((a) => <option key={a.number} value={a.number}>{a.number} {a.name}</option>)}
                    <optgroup label="Bedrijfsmiddelen (afschrijven via Activa)">
                      {(accounts.data ?? []).filter((a) => a.account_type !== "expense").map((a) => <option key={a.number} value={a.number}>{a.number} {a.name}</option>)}
                    </optgroup>
                    {l.account && accounts.data && !accounts.data.some((a) => a.number === l.account) &&
                      <option value={l.account}>{l.account} — geen kostenrekening, kies een andere</option>}
                  </select>
                </td>
                <td>
                  <select value={l.tax_code} onChange={(e) => setLine(i, { tax_code: e.target.value, vat: suggestVat(l.net, e.target.value) })}>
                    {purchaseCodes.map((t) => <option key={t.code} value={t.code}>{t.code} {t.name}</option>)}
                  </select>
                </td>
                <td><input className="num" value={l.net} onChange={(e) => setLine(i, { net: e.target.value, vat: suggestVat(e.target.value, l.tax_code) })} /></td>
                <td><input className="num" value={l.vat} onChange={(e) => setLine(i, { vat: e.target.value })} title="Neem het btw-bedrag van de factuur over" /></td>
                <td>{form.lines.length > 1 && <button onClick={() => setForm({ ...form, lines: form.lines.filter((_, j) => j !== i) })}>×</button>}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr><td colSpan={2}><button onClick={() => setForm({ ...form, lines: [...form.lines, emptyLine(form.lines[0]?.tax_code ?? "I21")] })}>+ Regel</button></td>
              <td className="num" colSpan={2}><strong>Te betalen {formatEuro(total)}</strong></td><td /></tr>
          </tfoot>
        </table>
        <label className="check"><input type="checkbox" checked={form.paid_privately} onChange={(e) => setForm({ ...form, paid_privately: e.target.checked })} /> Betaald vanaf privérekening (boekt op Privé-stortingen)</label>
        {error && <p className="error">{error}</p>}
        <div className="row gap" style={{ marginTop: 12 }}>
          <button className="primary" onClick={save}>{correcting ? "Correctie opslaan en boeken" : "Registreren en boeken"}</button>
          {docs.length === 0 && <span className="warning small">Nog geen bewijsstuk toegevoegd.</span>}
        </div>
      </div>
      <aside className="panel">
        <h3>Bewijsstukken</h3>
        {docs.length === 0 && <p className="muted small">Voeg de factuur, bon of andere stukken toe (PDF of foto, meerdere tegelijk mogelijk); je ziet ze hier naast de gegevens.</p>}
        <DocumentUpload type="purchase_invoice" multiple label={docs.length ? "Meer bestanden…" : "Bestanden kiezen…"}
          onUploaded={(added) => {
            // The first file of a new invoice is read automatically; later files only on request.
            if (docs.length === 0 && !correcting && added[0]) readDocument(added[0]);
            setDocs((cur) => [...cur, ...added.filter((a) => !cur.some((c) => c.id === a.id))]);
          }} />
        {docs.length > 0 && (
          <button disabled={reading} onClick={() => readDocument(docs[0])} title="Vult alleen lege velden in; je controleert alles voordat je boekt">
            Gegevens uit bestand lezen
          </button>
        )}
        <DocumentTabs docs={docs} height={560} onRemove={(rid) => setDocs((cur) => cur.filter((c) => c.id !== rid))} />
      </aside>
    </section>
  );
}
