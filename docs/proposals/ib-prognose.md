# Proposal: IB-prognose (income-tax estimate)

**Status: pending_approval** (proposed 2026-11-12; nothing implemented yet)

Implements handover §5 task 1. This file records the parameters and the calculation order so the
reasoning survives even if the design changes; it becomes `docs/accounting-rules.md` text once approved.

## 1. Parameters 2026 (final, verified on belastingdienst.nl on 2026-11-12)

All for an entrepreneur **under AOW age**. Rates stored as micro-units (`parse_rate_micro`: 35,75% = 357500),
amounts as integer cents.

| Key | Value | Source |
|---|---|---|
| Box 1 bracket 1: up to | € 38.883 at 35,75% | [belastingberekening 2026](https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/belastingberekening) |
| Box 1 bracket 2: up to | € 78.426 at 37,56% | same |
| Box 1 bracket 3: above | € 78.426 at 49,50% | same |
| Zelfstandigenaftrek | € 1.200 (urencriterium; capped at profit unless startersaftrek applies) | [zelfstandigenaftrek 2026](https://www.belastingdienst.nl/wps/wcm/connect/bldcontentnl/belastingdienst/zakelijk/winst/inkomstenbelasting/veranderingen-inkomstenbelasting-2026/ondernemersaftrek-2026/zelfstandigenaftrek-2026) |
| Startersaftrek | € 2.123 (not an entrepreneur in ≥1 of previous 5 years, ≤2× zelfstandigenaftrek used) | same |
| Mkb-winstvrijstelling | 12,7% of profit after ondernemersaftrek (no urencriterium) | [mkb-winstvrijstelling 2026](https://www.belastingdienst.nl/wps/wcm/connect/bldcontentnl/belastingdienst/zakelijk/winst/inkomstenbelasting/veranderingen-inkomstenbelasting-2026/mkb-winstvrijstelling-2026) |
| Max. rate for ondernemersaftrek + mkb-vrijstelling | 37,56% | both pages above |
| Algemene heffingskorting | max € 3.115; −6,398% of income above € 29.736; € 0 from € 78.426 | [heffingskortingen 2026](https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen) |
| Arbeidskorting | ≤ € 11.965: 8,324%; ≤ € 25.845: € 996 + 31,009%; ≤ € 45.592: € 5.300 + 1,950%; ≤ € 132.920: € 5.685 − 6,510%; above: € 0 | same |
| Zvw-bijdrage ondernemer | 4,85% up to max. bijdrage-inkomen € 79.409 | [percentages Zvw](https://www.belastingdienst.nl/wps/wcm/connect/bldcontentnl/belastingdienst/prive/werk_en_inkomen/zorgverzekeringswet/veranderingen-bijdrage-zvw/percentages-zvw) |

**2027 is not final**: Belastingplan 2027 was presented 15-09-2026 and is still in parliament. Proposal: add
2027 rows in a later migration once belastingdienst.nl publishes the final figures; until then a 2027 estimate
uses the 2026 parameters and the screen says so.

## 2. Calculation order (pure function in `boekhoud-core`, tests with the fixed values above)

1. Profit = year result from the ledger (revenue − costs, as on the dashboard). **[TOETSEN]** no fiscal
   corrections (e.g. limited deductibility of mixed costs) — accounting profit is treated as fiscal profit.
2. Ondernemersaftrek = zelfstandigenaftrek (if hours ≥ 1.225 from `hours_summary`) + startersaftrek (if the user
   ticks "starter"); zelfstandigenaftrek capped at profit unless startersaftrek applies. No carry-forward of
   unused deduction.
3. Mkb-winstvrijstelling = 12,7% × (profit − ondernemersaftrek).
4. Taxable profit + salary income (user input) = box 1 income → tax per bracket.
5. Rate cap: the part of (ondernemersaftrek + mkb-vrijstelling) that falls in bracket 3 is added back at
   49,50% − 37,56%.
6. Minus algemene heffingskorting (on box 1 income) and arbeidskorting (on salary + taxable profit),
   together at most the tax.
7. Plus Zvw 4,85% on taxable profit, up to (€ 79.409 − salary).
8. Minus loonheffing already withheld on the salary (user input, from payslip) → **amount still to pay**,
   and monthly reserve = that ÷ remaining months of the year.

Not in scope (shown as a note): box 2/3, partner and toeslagen, AOW age, eigen woning, the "more than 50% of
working time in the business" test (does not apply in the first 5 years for starters), the final assessment.

## 3. Storage and UI

- Migration `0008_tax_parameters.sql`: table `tax_parameter(year, key, value, source_url)` + the 2026 rows.
- Inputs salary, withheld loonheffing and "starter" are entered on the screen (not stored in v1).
- New screen **Belastingen → IB-prognose** with the steps of §2 as a table and the warning
  "winst in de boekhouding ≠ fiscale winst ≠ belastbaar inkomen ≠ definitieve aanslag".
