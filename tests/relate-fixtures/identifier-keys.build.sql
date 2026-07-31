-- Identifier-keyed relate fixture: raw registry column names, no `_id` rename.
--
-- Registries and a link table joined on columns spelled `lei` and `isin` —
-- names carrying NO `id`/`_id`/`_key`/`code` ending, so the name-spelling
-- heuristic scores them 0.2 and every edge here lands at 0.44 against the
-- 0.60 auto-accept line. Every join key is a checksum-valid identifier, so
-- finetype can type the columns and relate can score the edge on the types
-- instead of on the spelling.
--
-- Values are synthetic, generated to satisfy ISO 17442 (LEI, ISO 7064 mod
-- 97-10) and ISO 6166 (ISIN, Luhn) — the check digits are real, the entities
-- are not. Expected outcomes: identifier-keys.manifest.json.

-- (a) RAW-NAMED KEYS -> accepted, with NO rename.
--     link.lei  ⊆ entity_registry.lei   (unique parent, 0 orphans)
--     link.isin ⊆ security_registry.isin
CREATE TABLE entity_registry (lei VARCHAR, legal_name VARCHAR, jurisdiction VARCHAR);
INSERT INTO entity_registry VALUES('3174JD7PZ7LQDDW4V373','Aldgate Holdings','GB');
INSERT INTO entity_registry VALUES('694842OJFTJYF0TWN800','Brookfield Mutual','DE');
INSERT INTO entity_registry VALUES('7830RFI6ZNEKKSHMTM20','Cavendish Partners','FR');
INSERT INTO entity_registry VALUES('95311UM6WKBY7XQDH639','Dunbar Industrial','US');
INSERT INTO entity_registry VALUES('4274YVMT1K5KNQG6D637','Eastgate Systems','JP');
INSERT INTO entity_registry VALUES('44523NKSD1FLNK0D3T00','Fairhaven Group','CA');
INSERT INTO entity_registry VALUES('50349EOSTQ7MC7COR571','Garrick Logistics','NL');
INSERT INTO entity_registry VALUES('96702H7ZJ6TYB8A9Y813','Holloway Materials','IE');
INSERT INTO entity_registry VALUES('9047RWZDP5RYS1FZ4E20','Inverness Chemical','GB');
INSERT INTO entity_registry VALUES('26761ML3WFS2FRDYFE43','Jarrow Foods','DE');
INSERT INTO entity_registry VALUES('48244FOZWYLQ16ST5T26','Kelvinside Energy','FR');
INSERT INTO entity_registry VALUES('6030UGRR6S6V4SMNTE65','Lambourne Media','US');
INSERT INTO entity_registry VALUES('9434Y5VOTRZYXYNB5850','Marchmont Metals','JP');
INSERT INTO entity_registry VALUES('7765ZPIHFM93EPD56N72','Newhaven Marine','CA');
INSERT INTO entity_registry VALUES('8227LVRML7NT4MTDVH48','Oakworth Textiles','NL');
INSERT INTO entity_registry VALUES('7358U91PKDZYSZ4YZO50','Pentland Aviation','IE');
INSERT INTO entity_registry VALUES('6904ILOEMCMRHIVL0Y93','Quarrymount Cement','GB');
INSERT INTO entity_registry VALUES('2986F42ZWSKUCCCSVC58','Ravensbourne Rail','DE');
INSERT INTO entity_registry VALUES('3256WQRDRG8C1BWC8K29','Southgate Optical','FR');
INSERT INTO entity_registry VALUES('20442KC4M6NYHEWEJZ39','Tollcross Paper','US');
INSERT INTO entity_registry VALUES('2409FRY1E6K6Z1MXLK19','Ullswater Marine','JP');
INSERT INTO entity_registry VALUES('1358PZNF675QT12X4W26','Ventnor Robotics','CA');
INSERT INTO entity_registry VALUES('6649KCXTT7PPHQ7B9684','Wharfedale Audio','NL');
INSERT INTO entity_registry VALUES('6182PF31C49WHYWVB279','Xenia Instruments','IE');
INSERT INTO entity_registry VALUES('67265CHEX3S4JQQCD204','Yarrow Shipping','GB');
INSERT INTO entity_registry VALUES('877441V0Q80SAZAZF556','Zetland Analytics','DE');
INSERT INTO entity_registry VALUES('15167WFY8D4WJ6G2GJ49','Ardmore Cabling','FR');
INSERT INTO entity_registry VALUES('83101VOQ7Q16NRT17S85','Balmoral Pumps','US');
INSERT INTO entity_registry VALUES('4500XKYGOMOQF1Q61Y47','Carrick Composites','JP');
INSERT INTO entity_registry VALUES('927533DCS8Q8YXO8FD42','Deveron Glass','CA');

CREATE TABLE security_registry (isin VARCHAR, instrument_name VARCHAR);
INSERT INTO security_registry VALUES('GB4O2P5VWZV8','Aldgate Holdings Ord Shares');
INSERT INTO security_registry VALUES('JPSTKPB2UGR1','Brookfield Mutual Pref Shares');
INSERT INTO security_registry VALUES('US3NPPPSIQX8','Cavendish Partners Class A Ord');
INSERT INTO security_registry VALUES('FRE1YWYFNL04','Dunbar Industrial Snr Notes 2031');
INSERT INTO security_registry VALUES('DEPVX278O4O6','Eastgate Systems Conv Bond 2029');
INSERT INTO security_registry VALUES('DE8S1ODMWNT3','Fairhaven Group Ord Shares B');
INSERT INTO security_registry VALUES('USOMHK8JPIL3','Garrick Logistics Ord Shares');
INSERT INTO security_registry VALUES('GBXQJ4Z1S177','Holloway Materials Pref Shares');
INSERT INTO security_registry VALUES('JP7ZCLSNSGR5','Inverness Chemical Class A Ord');
INSERT INTO security_registry VALUES('US0D46J24HX3','Jarrow Foods Snr Notes 2031');
INSERT INTO security_registry VALUES('CAJSUT28IMG4','Kelvinside Energy Conv Bond 2029');
INSERT INTO security_registry VALUES('JP18QUQBEE41','Lambourne Media Ord Shares B');
INSERT INTO security_registry VALUES('CA31X2I95HJ0','Marchmont Metals Ord Shares');
INSERT INTO security_registry VALUES('GBGVDWMIYZE2','Newhaven Marine Pref Shares');
INSERT INTO security_registry VALUES('DEXLSFYXTS02','Oakworth Textiles Class A Ord');
INSERT INTO security_registry VALUES('DEQF4HV0O734','Pentland Aviation Snr Notes 2031');
INSERT INTO security_registry VALUES('JPFTE648VMD2','Quarrymount Cement Conv Bond 2029');
INSERT INTO security_registry VALUES('FR80BE9VJVP4','Ravensbourne Rail Ord Shares B');
INSERT INTO security_registry VALUES('CAQB2JVOCNU6','Southgate Optical Ord Shares');
INSERT INTO security_registry VALUES('CATJ39FRNYF1','Tollcross Paper Pref Shares');
INSERT INTO security_registry VALUES('USYGNC25PVK7','Ullswater Marine Class A Ord');
INSERT INTO security_registry VALUES('JPLPLU5BIQH2','Ventnor Robotics Snr Notes 2031');
INSERT INTO security_registry VALUES('DEHJUK1KR3X2','Wharfedale Audio Conv Bond 2029');
INSERT INTO security_registry VALUES('USUN9TM7GB70','Xenia Instruments Ord Shares B');
INSERT INTO security_registry VALUES('JP4WVPFMRJH6','Yarrow Shipping Ord Shares');
INSERT INTO security_registry VALUES('FR7ZR1MGYLJ5','Zetland Analytics Pref Shares');
INSERT INTO security_registry VALUES('DEE389HS1771','Ardmore Cabling Class A Ord');
INSERT INTO security_registry VALUES('CANOTOB7FBA4','Balmoral Pumps Snr Notes 2031');
INSERT INTO security_registry VALUES('USGO2Y4UYNX3','Carrick Composites Conv Bond 2029');
INSERT INTO security_registry VALUES('JPHRMZRMEBD2','Deveron Glass Ord Shares B');

-- Both registries are supersets of the linked subset, as registries are: 20
-- of 30 entities matched to 20 of 30 securities.
CREATE TABLE link (lei VARCHAR, isin VARCHAR, method VARCHAR);
INSERT INTO link VALUES('3174JD7PZ7LQDDW4V373','GB4O2P5VWZV8','exact_name');
INSERT INTO link VALUES('694842OJFTJYF0TWN800','JPSTKPB2UGR1','exact_name');
INSERT INTO link VALUES('7830RFI6ZNEKKSHMTM20','US3NPPPSIQX8','exact_name');
INSERT INTO link VALUES('95311UM6WKBY7XQDH639','FRE1YWYFNL04','exact_name');
INSERT INTO link VALUES('4274YVMT1K5KNQG6D637','DEPVX278O4O6','exact_name');
INSERT INTO link VALUES('44523NKSD1FLNK0D3T00','DE8S1ODMWNT3','exact_name');
INSERT INTO link VALUES('50349EOSTQ7MC7COR571','USOMHK8JPIL3','exact_name');
INSERT INTO link VALUES('96702H7ZJ6TYB8A9Y813','GBXQJ4Z1S177','exact_name');
INSERT INTO link VALUES('9047RWZDP5RYS1FZ4E20','JP7ZCLSNSGR5','exact_name');
INSERT INTO link VALUES('26761ML3WFS2FRDYFE43','US0D46J24HX3','exact_name');
INSERT INTO link VALUES('48244FOZWYLQ16ST5T26','CAJSUT28IMG4','exact_name');
INSERT INTO link VALUES('6030UGRR6S6V4SMNTE65','JP18QUQBEE41','exact_name');
INSERT INTO link VALUES('9434Y5VOTRZYXYNB5850','CA31X2I95HJ0','exact_name');
INSERT INTO link VALUES('7765ZPIHFM93EPD56N72','GBGVDWMIYZE2','exact_name');
INSERT INTO link VALUES('8227LVRML7NT4MTDVH48','DEXLSFYXTS02','exact_name');
INSERT INTO link VALUES('7358U91PKDZYSZ4YZO50','DEQF4HV0O734','exact_name');
INSERT INTO link VALUES('6904ILOEMCMRHIVL0Y93','JPFTE648VMD2','exact_name');
INSERT INTO link VALUES('2986F42ZWSKUCCCSVC58','FR80BE9VJVP4','exact_name');
INSERT INTO link VALUES('3256WQRDRG8C1BWC8K29','CAQB2JVOCNU6','exact_name');
INSERT INTO link VALUES('20442KC4M6NYHEWEJZ39','CATJ39FRNYF1','exact_name');

-- (b) NAMES SHARE NOTHING, TYPES SHARE EVERYTHING -> accepted.
--     filings.entity_ref holds LEIs and points at entity_registry.lei. The
--     two names have no substring in common, so name similarity is 0.0 and
--     the pair was dropped by the candidate prune before it could be scored
--     at all. An entity files more than once, so entity_ref is NOT unique —
--     which keeps anything pointing back INTO filings out of `accepted`.
CREATE TABLE filings (entity_ref VARCHAR, filed_on VARCHAR, form VARCHAR);
INSERT INTO filings VALUES('3174JD7PZ7LQDDW4V373','2025-01-10','AR1');
INSERT INTO filings VALUES('694842OJFTJYF0TWN800','2025-02-11','AR2');
INSERT INTO filings VALUES('7830RFI6ZNEKKSHMTM20','2025-03-12','AR3');
INSERT INTO filings VALUES('95311UM6WKBY7XQDH639','2025-04-13','AR1');
INSERT INTO filings VALUES('4274YVMT1K5KNQG6D637','2025-05-14','AR2');
INSERT INTO filings VALUES('44523NKSD1FLNK0D3T00','2025-06-15','AR3');
INSERT INTO filings VALUES('67265CHEX3S4JQQCD204','2025-07-16','AR1');
INSERT INTO filings VALUES('877441V0Q80SAZAZF556','2025-08-17','AR2');
INSERT INTO filings VALUES('15167WFY8D4WJ6G2GJ49','2025-09-18','AR3');
INSERT INTO filings VALUES('83101VOQ7Q16NRT17S85','2025-01-10','AR1');
INSERT INTO filings VALUES('4500XKYGOMOQF1Q61Y47','2025-02-11','AR2');
INSERT INTO filings VALUES('927533DCS8Q8YXO8FD42','2025-03-12','AR3');
INSERT INTO filings VALUES('3174JD7PZ7LQDDW4V373','2026-01-10','AR1');
INSERT INTO filings VALUES('694842OJFTJYF0TWN800','2026-02-11','AR2');
INSERT INTO filings VALUES('7830RFI6ZNEKKSHMTM20','2026-03-12','AR3');
INSERT INTO filings VALUES('95311UM6WKBY7XQDH639','2026-04-13','AR1');
INSERT INTO filings VALUES('4274YVMT1K5KNQG6D637','2026-05-14','AR2');
INSERT INTO filings VALUES('44523NKSD1FLNK0D3T00','2026-06-15','AR3');
INSERT INTO filings VALUES('67265CHEX3S4JQQCD204','2026-07-16','AR1');
INSERT INTO filings VALUES('877441V0Q80SAZAZF556','2026-08-17','AR2');
INSERT INTO filings VALUES('15167WFY8D4WJ6G2GJ49','2026-09-18','AR3');
INSERT INTO filings VALUES('83101VOQ7Q16NRT17S85','2026-01-10','AR1');
INSERT INTO filings VALUES('4500XKYGOMOQF1Q61Y47','2026-02-11','AR2');
INSERT INTO filings VALUES('927533DCS8Q8YXO8FD42','2026-03-12','AR3');

-- (c) SEMANTIC MISMATCH -> suggested. Demoted, NOT pruned, NOT rejected.
--     mixed_ref.security_ref holds 3 LEIs. instrument_ref.security_ref is a
--     unique column of 30 ISINs plus those same 3 LEIs, so it types as ISIN
--     (30/33 = 91% of the sample, over the 90% agreement bar) while the child
--     types as LEI. Referential integrity holds, the parent is unique and
--     confidence clears the bar — all three accept gates pass — and the types
--     disagree, so the edge is held for review with both types recorded
--     rather than accepted silently. A different instrument universe from
--     security_registry, so nothing else joins to it.
CREATE TABLE instrument_ref (security_ref VARCHAR);
INSERT INTO instrument_ref VALUES('FRGVYEXY2K71');
INSERT INTO instrument_ref VALUES('JPMCH8OENZI5');
INSERT INTO instrument_ref VALUES('US4VLX2ULHX7');
INSERT INTO instrument_ref VALUES('GBO2E3W4SR54');
INSERT INTO instrument_ref VALUES('CAWETO7OQR50');
INSERT INTO instrument_ref VALUES('CAWF4UDJWLN2');
INSERT INTO instrument_ref VALUES('JP3F8ZY0BZ35');
INSERT INTO instrument_ref VALUES('USQ6I8Z1OVF3');
INSERT INTO instrument_ref VALUES('CA14M3F73UR1');
INSERT INTO instrument_ref VALUES('CAQPB4DREGH1');
INSERT INTO instrument_ref VALUES('FRPN2YWAPLI8');
INSERT INTO instrument_ref VALUES('USS17H6DAVX7');
INSERT INTO instrument_ref VALUES('CAYMHBTAJKM8');
INSERT INTO instrument_ref VALUES('GBHNQ1U2TXT2');
INSERT INTO instrument_ref VALUES('JP47NWPAIPE9');
INSERT INTO instrument_ref VALUES('DE3LGXQJK9P2');
INSERT INTO instrument_ref VALUES('CAN56A9DJO53');
INSERT INTO instrument_ref VALUES('DE0HOR2B6735');
INSERT INTO instrument_ref VALUES('DEKHCJF0IWI1');
INSERT INTO instrument_ref VALUES('CA9EK1M53D65');
INSERT INTO instrument_ref VALUES('JPVHZN75QMR8');
INSERT INTO instrument_ref VALUES('GBETKNPBVJ41');
INSERT INTO instrument_ref VALUES('CAPCBOFWNOZ2');
INSERT INTO instrument_ref VALUES('USXEMTMCV9R5');
INSERT INTO instrument_ref VALUES('JPE7JO7B0EK8');
INSERT INTO instrument_ref VALUES('DELABHBII5B3');
INSERT INTO instrument_ref VALUES('JPO5XE92QXQ1');
INSERT INTO instrument_ref VALUES('USRSXW7ZY2O4');
INSERT INTO instrument_ref VALUES('GBQMLG2SHZJ7');
INSERT INTO instrument_ref VALUES('FRVMDHI00UU3');
INSERT INTO instrument_ref VALUES('11718Q2LSQVZVA3FBS22');
INSERT INTO instrument_ref VALUES('6617OA3ZNV2ROLK06828');
INSERT INTO instrument_ref VALUES('3824OGHNXJA08018UL71');

CREATE TABLE mixed_ref (security_ref VARCHAR);
INSERT INTO mixed_ref VALUES('11718Q2LSQVZVA3FBS22');
INSERT INTO mixed_ref VALUES('6617OA3ZNV2ROLK06828');
INSERT INTO mixed_ref VALUES('3824OGHNXJA08018UL71');
