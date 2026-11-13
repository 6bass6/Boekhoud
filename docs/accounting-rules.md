# Boekingsregels en rekeningschema

> **Status: concept — ter beoordeling door een Nederlandse boekhouder/fiscalist.**
> Alle punten gemarkeerd met **[TOETSEN]** zijn aannames die vóór productiegebruik bevestigd moeten worden.
> Dit document is bewust in het Nederlands geschreven zodat een boekhouder het direct kan lezen.

## 1. Uitgangspunten

- Dubbel boekhouden; boekingen in euro's, bedragen in hele centen.
- Btw-stelsel: **factuurstelsel** (standaard). **[TOETSEN]**
- Btw-regime is configureerbaar met ingangsdatum: `regulier` of `KOR`. Zie §5.
- Dagboeken: `BNK` bank, `VRK` verkoop, `INK` inkoop, `MEM` memoriaal, `PLT` platformen.
  Boekstuknummers zijn doorlopend en zonder gaten per dagboek per boekjaar.
- Definitieve boekingen worden nooit gewijzigd; correctie = tegenboeking + nieuwe boeking.
- Het boekjaar is het kalenderjaar.

## 2. Rekeningschema (concept)

Eigen nummering volgens het decimale stelsel. Iedere rekening krijgt een `rgs_code`
(Referentie GrootboekSchema) zodra de boekhouder de koppeling heeft bevestigd. **[TOETSEN: RGS-koppeling]**

### Balans

| Nr | Naam | Type |
|----|------|------|
| 0100 | Computers en apparatuur | Activa |
| 0110 | Cumulatieve afschrijving computers en apparatuur | Activa (contra) |
| 0500 | Ondernemingsvermogen | Eigen vermogen |
| 0510 | Privé-stortingen | Eigen vermogen |
| 0520 | Privé-onttrekkingen | Eigen vermogen |
| 0590 | Resultaat lopend boekjaar | Eigen vermogen |
| 1100 | Bank zakelijk | Activa |
| 1300 | Debiteuren | Activa |
| 1310 | Te ontvangen van Apple | Activa |
| 1320 | Te ontvangen van Google | Activa |
| 1330 | Te ontvangen van advertentieplatformen | Activa |
| 1500 | Te vorderen btw (voorbelasting) | Activa |
| 1510 | Te betalen btw hoog tarief | Passiva |
| 1511 | Te betalen btw laag tarief | Passiva |
| 1515 | Te betalen btw verlegd (inkoop buitenland) | Passiva |
| 1520 | Btw-afdracht / rekening-courant Belastingdienst | Passiva |
| 1600 | Crediteuren | Passiva |
| 1700 | Vooruitbetaalde kosten | Activa |
| 1710 | Nog te betalen kosten | Passiva |
| 2000 | Vraagposten (nog uit te zoeken) | Activa |

### Winst-en-verliesrekening

| Nr | Naam | Type |
|----|------|------|
| 4000 | Hosting en cloud | Kosten |
| 4010 | Software en abonnementen | Kosten |
| 4020 | Ontwikkelaarslidmaatschappen (Apple, Google) | Kosten |
| 4100 | Marketing en advertenties | Kosten |
| 4200 | Reis- en verblijfkosten | Kosten |
| 4300 | Kantoorkosten | Kosten |
| 4400 | Telefoon en internet | Kosten |
| 4500 | Representatie (beperkt aftrekbaar) | Kosten |
| 4600 | Opleiding en vakliteratuur | Kosten |
| 4700 | Bankkosten | Kosten |
| 4800 | Afschrijvingen | Kosten |
| 4900 | Overige kosten | Kosten |
| 8000 | Omzet App Store (Apple) | Opbrengsten |
| 8010 | Omzet Google Play | Opbrengsten |
| 8020 | Omzet advertenties | Opbrengsten |
| 8100 | Omzet lezingen | Opbrengsten |
| 8200 | Omzet digitale producten (direct) | Opbrengsten |
| 9000 | Koersverschillen | Financieel |
| 9100 | Rentebaten en -lasten | Financieel |

## 3. Boekingsscenario's

Notatie: D = debet, C = credit. Bedragen zijn voorbeelden.

### S1 — App Store-opbrengst (maandelijks financieel rapport)

Aanname: Apple Distribution International Ltd (Ierland) koopt de app als commissionair
en verkoopt aan de consument; Apple draagt de consumenten-btw af. De ontwikkelaar verricht een
dienst aan Apple. Plaats van dienst: Ierland → **btw verlegd, rubriek 3b, opgaaf ICP**. **[TOETSEN]**

Omzet = de opbrengst die Apple aan de ontwikkelaar verschuldigd is (*proceeds*, na commissie).
Brutoverkopen en commissie worden wel in de platformadministratie vastgelegd voor analyse,
maar niet als omzet en kosten geboekt. **[TOETSEN: netto vs. bruto]**

Rapport Apple-periode (fiscale maand Apple), proceeds omgerekend door Apple: € 850,00

| Rekening | D | C |
|---|---|---|
| 1310 Te ontvangen van Apple | 850,00 | |
| 8000 Omzet App Store (btw-code: *Dienst EU verlegd*) | | 850,00 |

Uitbetaling binnen ca. 45 dagen na einde Apple-periode: € 848,20 ontvangen

| Rekening | D | C |
|---|---|---|
| 1100 Bank zakelijk | 848,20 | |
| 9000 Koersverschillen | 1,80 | |
| 1310 Te ontvangen van Apple | | 850,00 |

Let op: Apple-periodes volgen Apple's fiscale kalender (4-4-5 weken) en vallen niet samen met
kalendermaanden. De boekingsdatum is de laatste dag van de Apple-periode. **[TOETSEN: toerekening bij jaarovergang]**

### S2 — Google Play-opbrengst

Als S1, met 1320 en 8010. Wederpartij Google Commerce Ltd (Ierland), rubriek 3b + ICP. **[TOETSEN]**

### S3 — Lezing aan Nederlandse opdrachtgever, regulier btw-regime

Lezingen zijn in beginsel belast met 21%. De btw-vrijstelling voor lezingen geldt alleen als de
lezing wordt gegeven door of in opdracht van een publiekrechtelijk lichaam, vereniging of stichting,
gericht op wetenschap of algemene ontwikkeling, tegen een kostendekkende vergoeding.
Voor een eenmanszaak met een winstgevend honorarium is dat doorgaans niet het geval. **[TOETSEN per opdracht]**

Factuur € 1.000 excl. btw:

| Rekening | D | C |
|---|---|---|
| 1300 Debiteuren | 1.210,00 | |
| 8100 Omzet lezingen (btw-code: *Hoog 21%*, rubriek 1a) | | 1.000,00 |
| 1510 Te betalen btw hoog | | 210,00 |

Betaling:

| Rekening | D | C |
|---|---|---|
| 1100 Bank zakelijk | 1.210,00 | |
| 1300 Debiteuren | | 1.210,00 |

### S4 — Lezing onder de KOR

Geen btw op de factuur; vermelding dat de KOR wordt toegepast. **[TOETSEN: verplichte factuurtekst]**

| Rekening | D | C |
|---|---|---|
| 1300 Debiteuren | 1.000,00 | |
| 8100 Omzet lezingen (btw-code: *KOR*) | | 1.000,00 |

### S5 — Lezing aan zakelijke opdrachtgever in ander EU-land

Btw verlegd, btw-id van de afnemer op de factuur, rubriek 3b + ICP.
Uitzondering: een lezing die toegang tot een evenement betreft kan in het land van het evenement belast zijn. **[TOETSEN]**

| Rekening | D | C |
|---|---|---|
| 1300 Debiteuren | 1.000,00 | |
| 8100 Omzet lezingen (btw-code: *Dienst EU verlegd*) | | 1.000,00 |

### S6 — Inkoop Nederlandse leverancier met btw (hosting € 121 incl. 21%)

| Rekening | D | C |
|---|---|---|
| 4000 Hosting en cloud | 100,00 | |
| 1500 Te vorderen btw (rubriek 5b) | 21,00 | |
| 1600 Crediteuren | | 121,00 |

Onder de KOR is de btw niet aftrekbaar: 4000 wordt dan 121,00 en 1500 vervalt.

### S7 — Inkoop dienst uit ander EU-land (btw naar mij verlegd), € 100

| Rekening | D | C |
|---|---|---|
| 4010 Software en abonnementen | 100,00 | |
| 1500 Te vorderen btw (rubriek 5b) | 21,00 | |
| 1515 Te betalen btw verlegd (rubriek 4b) | | 21,00 |
| 1600 Crediteuren | | 100,00 |

Dienst van buiten de EU (bijv. VS): idem, rubriek **4a**.
Onder de KOR blijft de verlegde btw naar verwachting verschuldigd zonder aftrek. **[TOETSEN]**

### S8 — Aanschaf bedrijfsmiddel (laptop € 2.420 incl. btw) en afschrijving

| Rekening | D | C |
|---|---|---|
| 0100 Computers en apparatuur | 2.000,00 | |
| 1500 Te vorderen btw | 420,00 | |
| 1600 Crediteuren | | 2.420,00 |

Maandelijkse afschrijving, 5 jaar lineair, restwaarde € 0: € 33,33 (laatste termijn vangt afronding op).

| Rekening | D | C |
|---|---|---|
| 4800 Afschrijvingen | 33,33 | |
| 0110 Cumulatieve afschrijving | | 33,33 |

**[TOETSEN: afschrijvingstermijn, KIA-drempel, gemengd gebruik]**

### S9 — Zakelijke kosten privé betaald / privé-opname

Zakelijke kosten betaald vanaf privérekening (€ 50 excl., € 10,50 btw):

| Rekening | D | C |
|---|---|---|
| 4300 Kantoorkosten | 50,00 | |
| 1500 Te vorderen btw | 10,50 | |
| 0510 Privé-stortingen | | 60,50 |

Privé-opname van de zakelijke rekening € 500:

| Rekening | D | C |
|---|---|---|
| 0520 Privé-onttrekkingen | 500,00 | |
| 1100 Bank zakelijk | | 500,00 |

### S10 — Btw-aangifte en betaling

Aan het eind van het tijdvak worden 1500, 1510, 1511 en 1515 overgeboekt naar 1520;
het saldo van 1520 is het te betalen of terug te vragen bedrag. Betaling: D 1520 / C 1100.

### S11 — Correctie

Een foutieve definitieve boeking wordt volledig tegengeboekt (zelfde regels, omgekeerde bedragen,
verwijzing naar de originele boeking) en daarna correct opnieuw geboekt.

### S12 — Jaarafsluiting

Saldi van W&V-rekeningen → 0590 Resultaat lopend boekjaar; daarna 0510, 0520 en 0590 → 0500.
Balansrekeningen worden als beginbalans doorgeboekt naar het nieuwe jaar.

## 4. Btw-codes (concept)

| Code | Omschrijving | Tarief | Rubriek |
|------|--------------|--------|---------|
| `V21` | Verkoop hoog | 21% | 1a |
| `V9` | Verkoop laag | 9% | 1b |
| `V0EU` | Dienst aan EU-ondernemer, verlegd | — | 3b + ICP |
| `V0NEU` | Dienst aan ondernemer buiten EU | — | **[TOETSEN]** |
| `VKOR` | Verkoop onder KOR | — | geen |
| `I21` | Inkoop NL hoog | 21% | 5b |
| `I9` | Inkoop NL laag | 9% | 5b |
| `IEU` | Inkoop dienst EU, verlegd | 21% | 4b + 5b |
| `INEU` | Inkoop dienst buiten EU, verlegd | 21% | 4a + 5b |
| `GEEN` | Buiten de btw / geen btw | — | — |

De codes staan als gegevens in de database (tabel `tax_code`) met geldigheidsperiode. De applicatie
weigert een boeking als een btw-code niet geldig is op de boekingsdatum, of niet past bij het btw-regime
van die datum: `V21`, `V9`, `I21`, `I9`, `IEU`, `INEU` alleen onder het reguliere regime, `VKOR` alleen
onder de KOR. **[TOETSEN: inkoop met verlegde btw onder de KOR heeft eigen codes nodig]**

## 5. KOR — analyse voor deze onderneming

- Omzetgrens € 20.000 per kalenderjaar, alleen voor **in Nederland belaste** omzet.
  Diensten die in een ander land belast zijn (zoals S1/S2 aan Apple/Google Ierland) tellen niet mee. **[TOETSEN]**
- Voordeel: geen btw op lezingen → goedkoper voor opdrachtgevers die zelf geen btw kunnen aftrekken
  (scholen, verenigingen, stichtingen); minder aangiftes.
- Nadeel: geen aftrek van voorbelasting op investeringen en kosten (laptop, software, reizen).
  Opdrachtgevers die zelf btw-ondernemer zijn hebben geen voordeel van de KOR.
- De applicatie ondersteunt beide regimes met ingangsdatum, zodat een overstap correct wordt verwerkt.

## 6. Hoe de applicatie deze regels toepast

- **Verkoopfacturen:** btw per btw-code over het totaal van de regels, afgerond op centen (half naar
  boven). Bij uitreiken: D 1300 / C omzet per rekening en code / C 1510 of 1511 met code.
  Creditnota = factuur met negatieve aantallen die verwijst naar de originele factuur; die is daarmee
  verrekend. Vóór uitreiken controleert de applicatie: adres, KvK-nummer en (behalve onder de KOR)
  btw-id van de onderneming, adres van de klant, en bij btw verlegd het btw-id van de klant.
- **Inkoopfacturen:** btw-bedrag van de factuur wordt overgenomen (voorstel = tarief × netto).
  Verlegde btw (IEU/INEU): D 1500 en C 1515 met dezelfde code; aan de leverancier is alleen netto
  verschuldigd. "Privé betaald" boekt de tegenrekening op 0510 in plaats van 1600.
- **Platformen:** per rapport één boeking op de laatste dag van de rapportperiode; per valuta het
  euro-bedrag uit Apple Payments (Google rapporteert al in euro). De uitbetaling via de bank vereffent
  1310/1320; een verschil gaat naar 9000 Koersverschillen.
- **Bank:** banktransacties zijn geen boekingen. Bij verwerken direct op een rekening met btw-code
  wordt de btw uit het bedrag gehaald (netto = bruto × 100 / (100 + tarief), afgerond).
- **Btw-aangifte:** alleen regels met btw-code tellen mee. Grondslag van verkoopcodes in de rubriek van
  de code, btw op de betaalrekening van de code; voorbelasting (5b) op de aftrekrekening. Afronding
  naar hele euro's in je voordeel. Vastleggen boekt de btw-saldi van de periode over naar 1520 en sluit
  de maanden af. **[TOETSEN: 1e voor diensten aan ondernemers buiten de EU]**
- **ICP:** grondslag van regels met een ICP-code, per afnemer (btw-id uit het contact van de factuur,
  of het contact dat aan het platform is gekoppeld).
- **Afschrijving:** lineair per maand vanaf de aanschafmaand, laatste maand vangt afronding op;
  D 4800 / C 0110. Percentage zakelijk gebruik wordt vastgelegd maar nog niet verwerkt.
  **[TOETSEN: privégebruik bedrijfsmiddelen]**
- **Uren:** urencriterium 1.225 uur per kalenderjaar; alle uren voor de onderneming tellen mee.
  Met een baan in loondienst geldt daarnaast dat je meer tijd aan de onderneming moet besteden dan
  aan de baan (lees de actuele voorwaarden bij de Belastingdienst). **[TOETSEN]**

## 7. IB-prognose (schatting inkomstenbelasting)

Scherm **Belastingen → IB-prognose**. Een schatting voor een ondernemer **onder de AOW-leeftijd**:
winst in de boekhouding ≠ fiscale winst ≠ belastbaar inkomen ≠ definitieve aanslag.

**Parameters zijn data**: tabel `tax_parameter` (jaar, sleutel, waarde, bron-URL), migratie
`0008_tax_parameters.sql`. Bedragen in centen, percentages in micro-eenheden (35,75% = 357500).
Een nieuw jaar komt erbij via een nieuwe migratie. Voor een jaar zonder parameters rekent de applicatie
met het laatste jaar dat wel parameters heeft en meldt dat op het scherm. **2027 is nog niet definitief**
(Belastingplan 2027 ligt bij het parlement); 2027-rijen worden toegevoegd zodra belastingdienst.nl de
definitieve cijfers publiceert.

Parameters 2026 (definitief, gecontroleerd op belastingdienst.nl op 12-11-2026):

| Parameter | Waarde | Bron |
|---|---|---|
| Schijf 1 box 1 | t/m € 38.883: 35,75% | [belastingberekening 2026](https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/belastingberekening) |
| Schijf 2 box 1 | t/m € 78.426: 37,56% | idem |
| Schijf 3 box 1 | daarboven: 49,50% | idem |
| Zelfstandigenaftrek | € 1.200 | [zelfstandigenaftrek 2026](https://www.belastingdienst.nl/wps/wcm/connect/bldcontentnl/belastingdienst/zakelijk/winst/inkomstenbelasting/veranderingen-inkomstenbelasting-2026/ondernemersaftrek-2026/zelfstandigenaftrek-2026) |
| Startersaftrek | € 2.123 | idem |
| Mkb-winstvrijstelling | 12,7% | [mkb-winstvrijstelling 2026](https://www.belastingdienst.nl/wps/wcm/connect/bldcontentnl/belastingdienst/zakelijk/winst/inkomstenbelasting/veranderingen-inkomstenbelasting-2026/mkb-winstvrijstelling-2026) |
| Max. tarief ondernemersaftrek + mkb-vrijstelling | 37,56% | beide pagina's hierboven |
| Algemene heffingskorting | € 3.115; − 6,398% van inkomen boven € 29.736; € 0 vanaf € 78.426 | [heffingskortingen 2026](https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen) |
| Arbeidskorting | t/m € 11.965: 8,324%; t/m € 25.845: € 996 + 31,009% boven € 11.965; t/m € 45.592: € 5.300 + 1,950% boven € 25.845; t/m € 132.920: € 5.685 − 6,510% boven € 45.592; daarboven € 0 | idem |
| Bijdrage Zvw ondernemer | 4,85% tot max. bijdrage-inkomen € 79.409 | [percentages Zvw](https://www.belastingdienst.nl/wps/wcm/connect/bldcontentnl/belastingdienst/prive/werk_en_inkomen/zorgverzekeringswet/veranderingen-bijdrage-zvw/percentages-zvw) |

Rekenvolgorde (`boekhoud_core::income_tax`, elke vermenigvuldiging afgerond op centen, half van nul af):

1. **Winst** = resultaat van het jaar uit de boekhouding (opbrengsten − kosten, zoals op het overzicht).
   **[TOETSEN]** Geen fiscale correcties (zoals beperkt aftrekbare gemengde kosten): de winst in de
   boekhouding geldt als fiscale winst.
2. **Ondernemersaftrek** = zelfstandigenaftrek (als de uren ≥ 1.225 volgens de urenregistratie) +
   startersaftrek (als de gebruiker "starter" aanvinkt). De zelfstandigenaftrek is beperkt tot de winst,
   behalve als er startersaftrek is (dan kan er een verlies ontstaan). Geen doorschuiven van niet-benutte
   aftrek. **[TOETSEN]** Startersaftrek alleen samen met het urencriterium (het is een verhoging van de
   zelfstandigenaftrek; de uitzondering bij arbeidsongeschiktheid is niet ingebouwd).
3. **Mkb-winstvrijstelling** = 12,7% × (winst − ondernemersaftrek); bij een verlies negatief
   (de vrijstelling verkleint het verlies). Geen urencriterium nodig.
4. **Box 1-inkomen** = belastbare winst + loon uit dienstbetrekking (eigen invoer); belasting per schijf.
   Een negatief box 1-inkomen wordt als € 0 belast (geen verliesverrekening met andere jaren).
5. **Aftrekbeperking**: het deel van (ondernemersaftrek + mkb-vrijstelling) dat in schijf 3 valt, wordt
   teruggeteld tegen 49,50% − 37,56% = 11,94%. Dat deel = de aftrek, maar hooguit het bedrag waarmee het
   inkomen vóór die aftrek boven de grens van schijf 3 uitkomt.
6. **Heffingskortingen**: algemene heffingskorting over het box 1-inkomen en arbeidskorting over
   loon + belastbare winst; samen hooguit de belasting. **[TOETSEN]** Vereenvoudiging: de kortingen worden
   afgezet tegen de hele box 1-belasting (in werkelijkheid alleen tegen het deel dat niet-AOW-gerechtigden
   betalen, en de algemene heffingskorting hangt af van het verzamelinkomen).
7. **Bijdrage Zvw** = 4,85% × belastbare winst, hooguit over (€ 79.409 − loon), omdat over het loon al Zvw
   via de werkgever wordt afgedragen.
8. **Nog te betalen** = totaal − al ingehouden loonheffing (eigen invoer, van loonstrook/jaaropgaaf);
   negatief = terug te ontvangen. **Reserve per maand** = nog te betalen ÷ resterende maanden van het jaar
   inclusief de huidige maand (geen advies voor een afgelopen jaar).

Loon, ingehouden loonheffing en "starter" worden op het scherm ingevuld en niet opgeslagen.

Niet meegenomen (staat als opmerking op het scherm): box 2 en 3, partner en toeslagen, AOW-leeftijd,
eigen woning, voorlopige aanslagen die al betaald worden, en de eis dat je met een baan meer dan de helft
van je werktijd aan de onderneming besteedt (geldt niet voor starters in de eerste 5 jaar).

## 8. Open vragen voor de boekhouder

1. Netto (proceeds) of bruto (verkopen − commissie) verantwoorden voor Apple/Google? (S1)
2. Bevestiging rubriek 3b + ICP voor Apple- en Google-opbrengsten; behandeling bij KOR.
3. Toerekening van Apple-periodes rond de jaarovergang.
4. Rubriek voor diensten aan ondernemers buiten de EU (bijv. advertentieplatform in de VS).
5. RGS-koppeling van het rekeningschema.
6. Afrondingsregel btw: per regel of per factuurtotaal.
7. KOR of regulier voor het eerste jaar, gezien verwachte investeringen en type opdrachtgevers.
8. Afschrijvingstermijnen en toepassing KIA.
9. IB-prognose: winst in de boekhouding als fiscale winst, startersaftrek alleen met urencriterium en
   de vereenvoudigde verrekening van heffingskortingen (§7).
