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

## 6. Open vragen voor de boekhouder

1. Netto (proceeds) of bruto (verkopen − commissie) verantwoorden voor Apple/Google? (S1)
2. Bevestiging rubriek 3b + ICP voor Apple- en Google-opbrengsten; behandeling bij KOR.
3. Toerekening van Apple-periodes rond de jaarovergang.
4. Rubriek voor diensten aan ondernemers buiten de EU (bijv. advertentieplatform in de VS).
5. RGS-koppeling van het rekeningschema.
6. Afrondingsregel btw: per regel of per factuurtotaal.
7. KOR of regulier voor het eerste jaar, gezien verwachte investeringen en type opdrachtgevers.
8. Afschrijvingstermijnen en toepassing KIA.
