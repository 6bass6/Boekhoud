-- 0008: yearly income-tax parameters for the IB-prognose (entrepreneur under AOW age).
-- Amounts in cents, rates in micro-units (35,75% = 357500, see parse_rate_micro).
-- Indexed segments: box1_bracket_N_{upto,rate}; ahk_N_ / arbeidskorting_N_{upto,base,rate}.
-- A segment without _upto has no upper limit. Credit segment N = base + rate × (income − limit of N−1).
-- New years are added by a new migration; rows are never edited.
CREATE TABLE tax_parameter (
    year        INTEGER NOT NULL,
    key         TEXT NOT NULL,
    value       INTEGER NOT NULL,
    source_url  TEXT NOT NULL,
    PRIMARY KEY (year, key)
);

-- 2026, final figures, verified on belastingdienst.nl on 2026-11-12.
INSERT INTO tax_parameter (year, key, value, source_url) VALUES
 (2026, 'box1_bracket_1_upto', 3888300, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/belastingberekening'),
 (2026, 'box1_bracket_1_rate', 357500, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/belastingberekening'),
 (2026, 'box1_bracket_2_upto', 7842600, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/belastingberekening'),
 (2026, 'box1_bracket_2_rate', 375600, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/belastingberekening'),
 (2026, 'box1_bracket_3_rate', 495000, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/belastingberekening'),
 (2026, 'zelfstandigenaftrek', 120000, 'https://www.belastingdienst.nl/wps/wcm/connect/bldcontentnl/belastingdienst/zakelijk/winst/inkomstenbelasting/veranderingen-inkomstenbelasting-2026/ondernemersaftrek-2026/zelfstandigenaftrek-2026'),
 (2026, 'startersaftrek', 212300, 'https://www.belastingdienst.nl/wps/wcm/connect/bldcontentnl/belastingdienst/zakelijk/winst/inkomstenbelasting/veranderingen-inkomstenbelasting-2026/ondernemersaftrek-2026/zelfstandigenaftrek-2026'),
 (2026, 'mkb_winstvrijstelling_rate', 127000, 'https://www.belastingdienst.nl/wps/wcm/connect/bldcontentnl/belastingdienst/zakelijk/winst/inkomstenbelasting/veranderingen-inkomstenbelasting-2026/mkb-winstvrijstelling-2026'),
 (2026, 'aftrek_max_rate', 375600, 'https://www.belastingdienst.nl/wps/wcm/connect/bldcontentnl/belastingdienst/zakelijk/winst/inkomstenbelasting/veranderingen-inkomstenbelasting-2026/mkb-winstvrijstelling-2026'),
 (2026, 'ahk_1_upto', 2973600, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'ahk_1_base', 311500, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'ahk_1_rate', 0, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'ahk_2_upto', 7842600, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'ahk_2_base', 311500, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'ahk_2_rate', -63980, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'ahk_3_base', 0, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'ahk_3_rate', 0, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'arbeidskorting_1_upto', 1196500, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'arbeidskorting_1_base', 0, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'arbeidskorting_1_rate', 83240, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'arbeidskorting_2_upto', 2584500, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'arbeidskorting_2_base', 99600, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'arbeidskorting_2_rate', 310090, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'arbeidskorting_3_upto', 4559200, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'arbeidskorting_3_base', 530000, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'arbeidskorting_3_rate', 19500, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'arbeidskorting_4_upto', 13292000, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'arbeidskorting_4_base', 568500, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'arbeidskorting_4_rate', -65100, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'arbeidskorting_5_base', 0, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'arbeidskorting_5_rate', 0, 'https://www.belastingdienst.nl/wps/wcm/connect/fisin/fisin2026/heffingskortingen'),
 (2026, 'zvw_rate', 48500, 'https://www.belastingdienst.nl/wps/wcm/connect/bldcontentnl/belastingdienst/prive/werk_en_inkomen/zorgverzekeringswet/veranderingen-bijdrage-zvw/percentages-zvw'),
 (2026, 'zvw_max_income', 7940900, 'https://www.belastingdienst.nl/wps/wcm/connect/bldcontentnl/belastingdienst/prive/werk_en_inkomen/zorgverzekeringswet/veranderingen-bijdrage-zvw/percentages-zvw');
