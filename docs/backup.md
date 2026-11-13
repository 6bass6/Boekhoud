# Back-ups, terugzetten en verhuizen naar een andere pc

Deze handleiding is voor de eigenaar. Alle gegevens staan in de **gegevensmap**
(`%APPDATA%\nl.boekhoud.app\`; het pad staat ook onder Instellingen → Back-up):

```
boekhoud.sqlite      de administratie (database)
documents\           bonnen, facturen en rapporten (bestandsnaam = controlesom, nooit gewijzigd)
backups\             back-ups (zie hieronder)
exports\             exports voor een andere pc (als je geen andere doelmap kiest)
backup-instellingen.json   de extra back-upmap
```

## 1. Wat de app automatisch doet

- **Bij het starten** maakt de app een back-up als de laatste goede automatische back-up ouder is dan
  24 uur. Dat gebeurt op de achtergrond; je kunt gewoon doorwerken. Een mislukte back-up houdt het
  starten nooit tegen.
- Een back-up bestaat uit:
  - `backups\auto-JJJJMMDD-UUMMSS.sqlite`: een consistente kopie van de database;
  - `backups\auto-JJJJMMDD-UUMMSS.json`: het controleresultaat;
  - `backups\documents\`: één gedeelde kopie van alle documenten. Documenten veranderen nooit, dus
    alleen nieuwe documenten worden erbij gekopieerd. Elke databasekopie hoort bij deze ene map.
- **Controle**: direct na het schrijven wordt de kopie alleen-lezen geopend en gecontroleerd:
  SQLite-integriteit (`PRAGMA integrity_check`), de hashketen van alle boekingen, en of elk document
  aanwezig is met de juiste controlesom. Het resultaat zie je onder Instellingen.
- **Bewaren**: de laatste 14 goede automatische back-ups blijven staan. Handmatige back-ups
  (`handmatig-…`) en kopieën van vóór een update (`voor-migratie-…`) worden nooit verwijderd. Een
  mislukte back-up zorgt er nooit voor dat een goede wordt verwijderd.
- **Extra back-upmap** (aanrader: een USB-stick of een OneDrive-map): vul onder Instellingen → Back-up
  een volledig pad in, bijvoorbeeld `E:\`. De app maakt daar de map `Boekhoud-back-ups` met dezelfde
  indeling en controle. Is de map niet bereikbaar (stick niet aangesloten), dan staat dat onder
  Instellingen en gaat de app gewoon door.
- Op het **Overzicht** verschijnt een aandachtspunt als de laatste goede back-up ouder is dan 7 dagen.
- Met **Nu back-up maken** maak je zelf een back-up (zelfde controle, ook naar de extra map).

Let op: de back-ups zijn **niet versleuteld**. Bewaar een USB-stick met back-ups op een veilige plek.

## 2. Een back-up terugzetten (handmatig)

Doe dit alleen als de administratie beschadigd of kwijt is. Werk rustig en gooi niets weg.

1. **Sluit Boekhoud.**
2. Kies onder Instellingen (of in de map `backups`) de **nieuwste back-up met controle "in orde"**.
   Weet je het niet meer: open het `.json`-bestand naast de back-up met Kladblok; daar moet
   `"ok": true` in staan.
3. Open de gegevensmap in Verkenner (`Windows-toets + R`, typ `%APPDATA%\nl.boekhoud.app`, Enter).
4. Maak een map `oud-JJJJMMDD` en **verplaats** daarin: `boekhoud.sqlite`, en als ze bestaan
   `boekhoud.sqlite-wal` en `boekhoud.sqlite-shm`. Zo blijft de oude toestand bewaard.
5. **Kopieer** het gekozen back-upbestand (bijv. `backups\auto-20261113-090000.sqlite`) naar de
   gegevensmap en hernoem de kopie naar `boekhoud.sqlite`.
6. Documenten: kopieer alles uit `backups\documents\` naar `documents\` en kies bij de vraag
   "bestand vervangen" voor **overslaan** (bestaande documenten zijn identiek).
   Zet je terug vanaf de extra back-upmap, gebruik dan `Boekhoud-back-ups\documents\`.
7. Start Boekhoud. Ga naar **Proef- en saldibalans** en kies **Integriteit controleren**; controleer ook een paar
   recente boekingen en de proefbalans.
8. Alles wat je ná het moment van de back-up hebt ingevoerd, moet je opnieuw invoeren.

Dat dit werkt, wordt automatisch getest (`backup_is_verified_and_restores_with_equal_balances` in
`crates/boekhoud-db/src/backup_tests.rs`: back-up terugzetten in een lege map, openen, proefbalans gelijk).

## 3. Verhuizen naar een andere pc

### Op de oude pc
1. Instellingen → **Verhuizen naar een andere pc** → vul eventueel een doelmap in (bijv. `E:\` voor
   een USB-stick; leeg = map `exports` in de gegevensmap) → **Administratie exporteren**.
2. De app maakt een map `boekhoud-export-JJJJMMDD-UUMMSS` met `boekhoud.sqlite`, de map `documents`
   en `manifest.json` (app-versie, schemaversie, datum, en per bestand de SHA-256-controlesom). De export
   wordt na het schrijven gecontroleerd; pas daarna krijgt de map zijn definitieve naam.
3. Kopieer die hele map naar de nieuwe pc (USB-stick, externe schijf). Werk daarna niet meer verder op
   de oude pc, anders lopen de administraties uiteen.

### Op de nieuwe pc
1. Installeer Boekhoud (minstens dezelfde versie als op de oude pc) en start het één keer. Voer nog
   **niets** in.
2. Instellingen → **Exportmap om te importeren**: het volledige pad van de gekopieerde map, bijv.
   `D:\boekhoud-export-20261113-090000` → **Administratie importeren**.
3. De app controleert eerst alles: de administratie op deze pc moet leeg zijn (geen boekingen,
   facturen, documenten enz.), alle controlesommen moeten kloppen, de database moet de integriteits- en
   hashketencontrole doorstaan, en de export mag niet uit een nieuwere app-versie komen. Bij een
   probleem wordt er niets overschreven en zie je een melding.
4. De lege database van de nieuwe pc wordt bewaard als `backups\voor-import-….sqlite`; de import
   wordt vastgelegd in het auditlog. Daarna laadt de app opnieuw met de geïmporteerde administratie.
5. Controleer de proefbalans (met **Integriteit controleren**) en stel de extra back-upmap opnieuw in.
