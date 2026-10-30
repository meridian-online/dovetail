# Role eval results

Each set's role calls scored against its answer key. Written by the `role_eval_harness` test with `DOVETAIL_WRITE_ROLE_RESULTS=1`; everything but the stamp is the same on every run of the same code.

Stamp: finetype 0.6.64, model sha256 86d7fcde7f92cbb2fb47bbe04ad65bfe991f9d698bf4d66c2a48e59fbf91fabe, dovetail 9bfcfef

## Summary

| set | tier | typing | tables | role accuracy | fold P / R | combination P / R |
|---|---|---|---|---|---|---|
| retail | 1 | floor | 12 | 12/12 (100.0%) | 1.00 / 1.00 | 1.00 / 1.00 |
| retail | 1 | profiled | 12 | 12/12 (100.0%) | 1.00 / 1.00 | 1.00 / 1.00 |

## Tier 1, floor

Rows are the answer key's role, columns the call.

| key \ call | entity | event | reference | bridge | snapshot | unclear |
|---|---|---|---|---|---|---|
| entity | 4 | 0 | 0 | 0 | 0 | 0 |
| event | 0 | 3 | 0 | 0 | 0 | 0 |
| reference | 0 | 0 | 1 | 0 | 0 | 0 |
| bridge | 0 | 0 | 0 | 0 | 0 | 0 |
| snapshot | 0 | 0 | 0 | 0 | 3 | 0 |
| unclear | 0 | 0 | 0 | 0 | 0 | 1 |

Misses: none.

## Tier 1, profiled

Rows are the answer key's role, columns the call.

| key \ call | entity | event | reference | bridge | snapshot | unclear |
|---|---|---|---|---|---|---|
| entity | 4 | 0 | 0 | 0 | 0 | 0 |
| event | 0 | 3 | 0 | 0 | 0 | 0 |
| reference | 0 | 0 | 1 | 0 | 0 | 0 |
| bridge | 0 | 0 | 0 | 0 | 0 | 0 |
| snapshot | 0 | 0 | 0 | 0 | 3 | 0 |
| unclear | 0 | 0 | 0 | 0 | 0 | 1 |

Misses: none.

# Corpus baseline: tiers 2 and 3

Tier 2 is each CTU database scored whole against its answer key; tier 3 is each sampled GitTables table scored alone. Written by the `role_corpus_eval` test's `corpus_baseline` from the corpus `scripts/fetch-role-corpus.sh` fetches; everything but the stamp is the same on every run of the same code over the same corpus.

Stamp: finetype 0.6.64, model sha256 86d7fcde7f92cbb2fb47bbe04ad65bfe991f9d698bf4d66c2a48e59fbf91fabe, dovetail 9e4acbb-dirty

## Tier 2 totals

| typing | databases | role accuracy | join recall |
|---|---|---|---|
| floor | 25 | 147/321 (45.8%) | 269/366 (73.5%) |
| profiled | 25 | 157/321 (48.9%) | 273/366 (74.6%) |

## Summary

| set | tier | typing | tables | role accuracy | fold P / R | combination P / R |
|---|---|---|---|---|---|---|
| AdventureWorks2014 | 2 | floor | 70 | 26/70 (37.1%) | - / 0.00 | - / 0.00 |
| Basketball_men | 2 | floor | 9 | 1/9 (11.1%) | - / - | - / - |
| Chinook | 2 | floor | 11 | 7/11 (63.6%) | 0.50 / 1.00 | - / - |
| Countries | 2 | floor | 4 | 1/4 (25.0%) | - / 0.00 | - / - |
| CraftBeer | 2 | floor | 2 | 2/2 (100.0%) | - / - | - / - |
| ErgastF1 | 2 | floor | 14 | 5/14 (35.7%) | - / 0.00 | - / - |
| GOSales | 2 | floor | 5 | 3/5 (60.0%) | - / 0.00 | - / - |
| Hockey | 2 | floor | 22 | 1/22 (4.5%) | - / - | - / - |
| Mondial | 2 | floor | 34 | 9/34 (26.5%) | - / - | 0.00 / - |
| PremierLeague | 2 | floor | 4 | 1/4 (25.0%) | 0.00 / - | - / - |
| TubePricing | 2 | floor | 20 | 15/20 (75.0%) | - / 0.00 | - / - |
| Walmart_2014 | 2 | floor | 3 | 1/3 (33.3%) | - / - | - / - |
| ccs | 2 | floor | 6 | 5/6 (83.3%) | 1.00 / 1.00 | - / - |
| classicmodels | 2 | floor | 8 | 6/8 (75.0%) | - / 0.00 | - / - |
| employee | 2 | floor | 6 | 1/6 (16.7%) | - / - | 0.00 / - |
| financial | 2 | floor | 8 | 6/8 (75.0%) | - / - | - / - |
| imdb_MovieLens | 2 | floor | 7 | 7/7 (100.0%) | - / - | - / - |
| imdb_small | 2 | floor | 7 | 2/7 (28.6%) | - / - | - / - |
| lahman_2014 | 2 | floor | 25 | 3/25 (12.0%) | - / - | - / - |
| northwind | 2 | floor | 13 | 9/13 (69.2%) | - / 0.00 | - / - |
| pubs | 2 | floor | 11 | 9/11 (81.8%) | - / 0.00 | - / - |
| sakila | 2 | floor | 16 | 13/16 (81.2%) | 1.00 / 0.67 | - / - |
| stats_CEB | 2 | floor | 8 | 7/8 (87.5%) | - / - | - / - |
| university | 2 | floor | 5 | 5/5 (100.0%) | - / - | - / - |
| world | 2 | floor | 3 | 2/3 (66.7%) | - / - | - / - |
| AdventureWorks2014 | 2 | profiled | 70 | 26/70 (37.1%) | - / 0.00 | - / 0.00 |
| Basketball_men | 2 | profiled | 9 | 4/9 (44.4%) | - / - | - / - |
| Chinook | 2 | profiled | 11 | 8/11 (72.7%) | 0.67 / 1.00 | - / - |
| Countries | 2 | profiled | 4 | 1/4 (25.0%) | - / 0.00 | - / - |
| CraftBeer | 2 | profiled | 2 | 2/2 (100.0%) | - / - | - / - |
| ErgastF1 | 2 | profiled | 14 | 6/14 (42.9%) | 1.00 / 0.50 | - / - |
| GOSales | 2 | profiled | 5 | 4/5 (80.0%) | 1.00 / 1.00 | - / - |
| Hockey | 2 | profiled | 22 | 4/22 (18.2%) | - / - | - / - |
| Mondial | 2 | profiled | 34 | 6/34 (17.6%) | - / - | - / - |
| PremierLeague | 2 | profiled | 4 | 1/4 (25.0%) | 0.00 / - | - / - |
| TubePricing | 2 | profiled | 20 | 16/20 (80.0%) | - / 0.00 | - / - |
| Walmart_2014 | 2 | profiled | 3 | 2/3 (66.7%) | - / - | - / - |
| ccs | 2 | profiled | 6 | 5/6 (83.3%) | 1.00 / 1.00 | - / - |
| classicmodels | 2 | profiled | 8 | 6/8 (75.0%) | - / 0.00 | - / - |
| employee | 2 | profiled | 6 | 2/6 (33.3%) | - / - | - / - |
| financial | 2 | profiled | 8 | 6/8 (75.0%) | - / - | - / - |
| imdb_MovieLens | 2 | profiled | 7 | 6/7 (85.7%) | - / - | - / - |
| imdb_small | 2 | profiled | 7 | 1/7 (14.3%) | - / - | - / - |
| lahman_2014 | 2 | profiled | 25 | 8/25 (32.0%) | - / - | - / - |
| northwind | 2 | profiled | 13 | 10/13 (76.9%) | - / 0.00 | - / - |
| pubs | 2 | profiled | 11 | 9/11 (81.8%) | - / 0.00 | - / - |
| sakila | 2 | profiled | 16 | 10/16 (62.5%) | 0.67 / 0.67 | - / - |
| stats_CEB | 2 | profiled | 8 | 7/8 (87.5%) | - / - | - / - |
| university | 2 | profiled | 5 | 5/5 (100.0%) | - / - | - / - |
| world | 2 | profiled | 3 | 2/3 (66.7%) | - / - | - / - |

## Tier 2, floor

Rows are the answer key's role, columns the call.

| key \ call | entity | event | reference | bridge | snapshot | unclear |
|---|---|---|---|---|---|---|
| entity | 95 | 9 | 4 | 0 | 1 | 18 |
| event | 9 | 27 | 0 | 0 | 0 | 19 |
| reference | 19 | 1 | 5 | 0 | 2 | 6 |
| bridge | 17 | 7 | 0 | 11 | 1 | 10 |
| snapshot | 11 | 5 | 0 | 1 | 4 | 28 |
| unclear | 1 | 0 | 0 | 0 | 0 | 5 |

Misses:

- AdventureWorks2014: combination `transactionhistory` (partition of transactionhistory, transactionhistoryarchive): in the key, not called
- AdventureWorks2014: fold `addresstype` into `businessentityaddress`: in the key, not called
- AdventureWorks2014: fold `contacttype` into `businessentitycontact`: in the key, not called
- AdventureWorks2014: fold `culture` into `productmodelproductdescriptionculture`: in the key, not called
- AdventureWorks2014: fold `department` into `employeedepartmenthistory`: in the key, not called
- AdventureWorks2014: fold `phonenumbertype` into `personphone`: in the key, not called
- AdventureWorks2014: fold `productcategory` into `productsubcategory`: in the key, not called
- AdventureWorks2014: fold `productdescription` into `productmodelproductdescriptionculture`: in the key, not called
- AdventureWorks2014: fold `productsubcategory` into `product`: in the key, not called
- AdventureWorks2014: fold `salesreason` into `salesorderheadersalesreason`: in the key, not called
- AdventureWorks2014: fold `scrapreason` into `workorder`: in the key, not called
- AdventureWorks2014: table `address`: the key says entity, called event
- AdventureWorks2014: table `addresstype`: the key says reference, called snapshot
- AdventureWorks2014: table `billofmaterials`: the key says snapshot, called event
- AdventureWorks2014: table `businessentityaddress`: the key says bridge, called event
- AdventureWorks2014: table `businessentitycontact`: the key says bridge, called event
- AdventureWorks2014: table `contacttype`: the key says reference, called entity
- AdventureWorks2014: table `countryregion`: the key says reference, called entity
- AdventureWorks2014: table `countryregioncurrency`: the key says bridge, called event
- AdventureWorks2014: table `culture`: the key says reference, called entity
- AdventureWorks2014: table `currency`: the key says reference, called entity
- AdventureWorks2014: table `currencyrate`: the key says snapshot, called entity
- AdventureWorks2014: table `customer`: the key says entity, called event
- AdventureWorks2014: table `databaselog`: the key says event, called unclear
- AdventureWorks2014: table `department`: the key says reference, called entity
- AdventureWorks2014: table `document`: the key says entity, called unclear
- AdventureWorks2014: table `employeedepartmenthistory`: the key says snapshot, called event
- AdventureWorks2014: table `employeepayhistory`: the key says snapshot, called event
- AdventureWorks2014: table `personcreditcard`: the key says bridge, called entity
- AdventureWorks2014: table `personphone`: the key says entity, called event
- AdventureWorks2014: table `phonenumbertype`: the key says reference, called entity
- AdventureWorks2014: table `product`: the key says entity, called event
- AdventureWorks2014: table `productcategory`: the key says reference, called snapshot
- AdventureWorks2014: table `productdescription`: the key says reference, called entity
- AdventureWorks2014: table `productdocument`: the key says bridge, called unclear
- AdventureWorks2014: table `productinventory`: the key says bridge, called event
- AdventureWorks2014: table `productmodelillustration`: the key says bridge, called unclear
- AdventureWorks2014: table `productmodelproductdescriptionculture`: the key says bridge, called snapshot
- AdventureWorks2014: table `productproductphoto`: the key says bridge, called event
- AdventureWorks2014: table `productreview`: the key says event, called unclear
- AdventureWorks2014: table `productsubcategory`: the key says reference, called event
- AdventureWorks2014: table `productvendor`: the key says bridge, called event
- AdventureWorks2014: table `salesorderheadersalesreason`: the key says bridge, called unclear
- AdventureWorks2014: table `salesperson`: the key says entity, called event
- AdventureWorks2014: table `salespersonquotahistory`: the key says snapshot, called event
- AdventureWorks2014: table `salesreason`: the key says reference, called entity
- AdventureWorks2014: table `salestaxrate`: the key says entity, called unclear
- AdventureWorks2014: table `salesterritory`: the key says entity, called unclear
- AdventureWorks2014: table `salesterritoryhistory`: the key says snapshot, called event
- AdventureWorks2014: table `scrapreason`: the key says reference, called entity
- AdventureWorks2014: table `shoppingcartitem`: the key says event, called entity
- AdventureWorks2014: table `specialofferproduct`: the key says bridge, called event
- AdventureWorks2014: table `stateprovince`: the key says entity, called event
- AdventureWorks2014: table `store`: the key says entity, called snapshot
- AdventureWorks2014: table `transactionhistoryarchive`: called event, not in the key
- AdventureWorks2014: table `unitmeasure`: the key says reference, called entity
- Basketball_men: table `awards_coaches`: the key says event, called unclear
- Basketball_men: table `awards_players`: the key says event, called unclear
- Basketball_men: table `coaches`: the key says snapshot, called unclear
- Basketball_men: table `draft`: the key says event, called entity
- Basketball_men: table `player_allstar`: the key says event, called entity
- Basketball_men: table `players_teams`: the key says snapshot, called entity
- Basketball_men: table `series_post`: the key says event, called unclear
- Basketball_men: table `teams`: the key says snapshot, called unclear
- Chinook: fold `artist` into `album`: called, not in the key
- Chinook: fold `playlist` into `playlisttrack`: called, not in the key
- Chinook: table `artist`: the key says entity, called reference
- Chinook: table `employee`: the key says entity, called unclear
- Chinook: table `invoiceline`: the key says event, called entity
- Chinook: table `playlist`: the key says entity, called reference
- Countries: fold `metadata - indicators` into `data`: in the key, not called
- Countries: table `data`: the key says snapshot, called entity
- Countries: table `metadata - indicators`: the key says reference, called unclear
- Countries: table `target`: the key says snapshot, called entity
- ErgastF1: fold `seasons` into `races`: in the key, not called
- ErgastF1: fold `status` into `results`: in the key, not called
- ErgastF1: table `constructorresults`: the key says event, called entity
- ErgastF1: table `constructorstandings`: the key says snapshot, called entity
- ErgastF1: table `driverstandings`: the key says snapshot, called entity
- ErgastF1: table `laptimes`: the key says event, called unclear
- ErgastF1: table `qualifying`: the key says event, called entity
- ErgastF1: table `results`: the key says event, called entity
- ErgastF1: table `seasons`: the key says reference, called entity
- ErgastF1: table `status`: the key says reference, called entity
- ErgastF1: table `target`: the key says snapshot, called unclear
- GOSales: fold `go_methods` into `go_daily_sales`: in the key, not called
- GOSales: table `go_methods`: the key says reference, called entity
- GOSales: table `go_products`: the key says entity, called unclear
- Hockey: table `abbrev`: the key says reference, called unclear
- Hockey: table `awardscoaches`: the key says event, called entity
- Hockey: table `awardsmisc`: the key says event, called unclear
- Hockey: table `awardsplayers`: the key says event, called unclear
- Hockey: table `coaches`: the key says snapshot, called unclear
- Hockey: table `combinedshutouts`: the key says event, called unclear
- Hockey: table `goalies`: the key says snapshot, called unclear
- Hockey: table `goaliessc`: the key says snapshot, called entity
- Hockey: table `goaliesshootout`: the key says snapshot, called unclear
- Hockey: table `master`: the key says entity, called unclear
- Hockey: table `scoring`: the key says snapshot, called unclear
- Hockey: table `scoringsc`: the key says snapshot, called entity
- Hockey: table `scoringshootout`: the key says snapshot, called unclear
- Hockey: table `scoringsup`: the key says snapshot, called entity
- Hockey: table `seriespost`: the key says event, called unclear
- Hockey: table `teams`: the key says snapshot, called unclear
- Hockey: table `teamshalf`: the key says snapshot, called unclear
- Hockey: table `teamsplits`: the key says snapshot, called unclear
- Hockey: table `teamspost`: the key says snapshot, called unclear
- Hockey: table `teamssc`: the key says snapshot, called unclear
- Hockey: table `teamvsteam`: the key says snapshot, called unclear
- Mondial: combination `geo` (partition of geo_estuary, geo_river, geo_source): called, not in the key
- Mondial: table `continent`: the key says entity, called unclear
- Mondial: table `desert`: the key says entity, called unclear
- Mondial: table `encompasses`: the key says bridge, called entity
- Mondial: table `ethnicgroup`: the key says bridge, called entity
- Mondial: table `geo_desert`: the key says bridge, called entity
- Mondial: table `geo_estuary`: in the key, not called
- Mondial: table `geo_island`: the key says bridge, called entity
- Mondial: table `geo_lake`: the key says bridge, called entity
- Mondial: table `geo_mountain`: the key says bridge, called entity
- Mondial: table `geo_river`: in the key, not called
- Mondial: table `geo_sea`: the key says bridge, called unclear
- Mondial: table `geo_source`: in the key, not called
- Mondial: table `geo`: called unclear, not in the key
- Mondial: table `island`: the key says entity, called unclear
- Mondial: table `islandin`: the key says bridge, called unclear
- Mondial: table `ismember`: the key says bridge, called entity
- Mondial: table `lake`: the key says entity, called unclear
- Mondial: table `language`: the key says bridge, called entity
- Mondial: table `located`: the key says bridge, called unclear
- Mondial: table `locatedon`: the key says bridge, called entity
- Mondial: table `mergeswith`: the key says bridge, called unclear
- Mondial: table `mountain`: the key says entity, called unclear
- Mondial: table `mountainonisland`: the key says bridge, called unclear
- Mondial: table `religion`: the key says bridge, called entity
- Mondial: table `river`: the key says entity, called unclear
- Mondial: table `sea`: the key says entity, called unclear
- PremierLeague: fold `players` into `actions`: called, not in the key
- PremierLeague: table `actions`: the key says event, called unclear
- PremierLeague: table `players`: the key says entity, called reference
- PremierLeague: table `teams`: the key says entity, called reference
- TubePricing: fold `tube_end_form` into `tube`: in the key, not called
- TubePricing: table `spec`: the key says bridge, called unclear
- TubePricing: table `tube_end_form`: the key says reference, called unclear
- TubePricing: table `type_component`: the key says reference, called entity
- TubePricing: table `type_connection`: the key says reference, called entity
- TubePricing: table `type_end_form`: the key says reference, called unclear
- Walmart_2014: table `stores`: the key says entity, called unclear
- Walmart_2014: table `train`: the key says snapshot, called unclear
- ccs: table `yearmonth`: the key says snapshot, called entity
- classicmodels: fold `productlines` into `products`: in the key, not called
- classicmodels: table `orderdetails`: the key says event, called unclear
- classicmodels: table `productlines`: the key says reference, called unclear
- employee: combination `dept` (partition of dept_emp, dept_manager): called, not in the key
- employee: table `departments`: the key says reference, called unclear
- employee: table `dept_emp`: in the key, not called
- employee: table `dept_manager`: in the key, not called
- employee: table `dept`: called unclear, not in the key
- employee: table `employees`: the key says entity, called unclear
- employee: table `titles`: the key says snapshot, called unclear
- financial: table `account`: the key says entity, called event
- financial: table `disp`: the key says bridge, called entity
- imdb_small: table `directors_genres`: the key says bridge, called entity
- imdb_small: table `movies_directors`: the key says bridge, called entity
- imdb_small: table `movies_genres`: the key says bridge, called unclear
- imdb_small: table `movies`: the key says entity, called unclear
- imdb_small: table `roles`: the key says bridge, called entity
- lahman_2014: table `allstarfull`: the key says event, called unclear
- lahman_2014: table `appearances`: the key says snapshot, called unclear
- lahman_2014: table `awardsmanagers`: the key says event, called unclear
- lahman_2014: table `awardsplayers`: the key says event, called unclear
- lahman_2014: table `awardssharemanagers`: the key says event, called unclear
- lahman_2014: table `awardsshareplayers`: the key says event, called unclear
- lahman_2014: table `batting`: the key says snapshot, called unclear
- lahman_2014: table `battingpost`: the key says snapshot, called unclear
- lahman_2014: table `els_teamnames`: the key says unclear, called entity
- lahman_2014: table `fielding`: the key says snapshot, called unclear
- lahman_2014: table `fieldingof`: the key says snapshot, called unclear
- lahman_2014: table `fieldingpost`: the key says snapshot, called unclear
- lahman_2014: table `halloffame`: the key says event, called entity
- lahman_2014: table `managers`: the key says snapshot, called unclear
- lahman_2014: table `managershalf`: the key says snapshot, called unclear
- lahman_2014: table `pitching`: the key says snapshot, called unclear
- lahman_2014: table `pitchingpost`: the key says snapshot, called unclear
- lahman_2014: table `salaries`: the key says snapshot, called unclear
- lahman_2014: table `schoolsplayers`: the key says snapshot, called bridge
- lahman_2014: table `seriespost`: the key says event, called unclear
- lahman_2014: table `teams`: the key says snapshot, called entity
- lahman_2014: table `teamshalf`: the key says snapshot, called unclear
- northwind: fold `categories` into `products`: in the key, not called
- northwind: fold `region` into `territories`: in the key, not called
- northwind: table `categories`: the key says reference, called entity
- northwind: table `order details`: the key says event, called unclear
- northwind: table `region`: the key says reference, called entity
- northwind: table `shippers`: the key says entity, called unclear
- pubs: fold `jobs` into `employee`: in the key, not called
- pubs: table `jobs`: the key says reference, called entity
- pubs: table `titles`: the key says entity, called event
- sakila: fold `country` into `city`: in the key, not called
- sakila: table `country`: the key says reference, called entity
- sakila: table `customer`: the key says entity, called event
- sakila: table `film_category`: the key says bridge, called entity
- stats_CEB: table `tags`: the key says entity, called unclear
- world: table `countrylanguage`: the key says bridge, called entity

## Tier 2, profiled

Rows are the answer key's role, columns the call.

| key \ call | entity | event | reference | bridge | snapshot | unclear |
|---|---|---|---|---|---|---|
| entity | 90 | 10 | 5 | 0 | 2 | 20 |
| event | 6 | 37 | 0 | 0 | 0 | 12 |
| reference | 15 | 1 | 9 | 0 | 1 | 7 |
| bridge | 17 | 12 | 0 | 9 | 0 | 11 |
| snapshot | 6 | 26 | 0 | 0 | 7 | 12 |
| unclear | 1 | 0 | 0 | 0 | 0 | 5 |

Misses:

- AdventureWorks2014: combination `transactionhistory` (partition of transactionhistory, transactionhistoryarchive): in the key, not called
- AdventureWorks2014: fold `addresstype` into `businessentityaddress`: in the key, not called
- AdventureWorks2014: fold `contacttype` into `businessentitycontact`: in the key, not called
- AdventureWorks2014: fold `culture` into `productmodelproductdescriptionculture`: in the key, not called
- AdventureWorks2014: fold `department` into `employeedepartmenthistory`: in the key, not called
- AdventureWorks2014: fold `phonenumbertype` into `personphone`: in the key, not called
- AdventureWorks2014: fold `productcategory` into `productsubcategory`: in the key, not called
- AdventureWorks2014: fold `productdescription` into `productmodelproductdescriptionculture`: in the key, not called
- AdventureWorks2014: fold `productsubcategory` into `product`: in the key, not called
- AdventureWorks2014: fold `salesreason` into `salesorderheadersalesreason`: in the key, not called
- AdventureWorks2014: fold `scrapreason` into `workorder`: in the key, not called
- AdventureWorks2014: table `address`: the key says entity, called event
- AdventureWorks2014: table `addresstype`: the key says reference, called entity
- AdventureWorks2014: table `billofmaterials`: the key says snapshot, called event
- AdventureWorks2014: table `businessentityaddress`: the key says bridge, called event
- AdventureWorks2014: table `businessentitycontact`: the key says bridge, called event
- AdventureWorks2014: table `contacttype`: the key says reference, called entity
- AdventureWorks2014: table `countryregion`: the key says reference, called entity
- AdventureWorks2014: table `countryregioncurrency`: the key says bridge, called event
- AdventureWorks2014: table `culture`: the key says reference, called entity
- AdventureWorks2014: table `currency`: the key says reference, called entity
- AdventureWorks2014: table `currencyrate`: the key says snapshot, called entity
- AdventureWorks2014: table `customer`: the key says entity, called event
- AdventureWorks2014: table `databaselog`: the key says event, called unclear
- AdventureWorks2014: table `department`: the key says reference, called entity
- AdventureWorks2014: table `document`: the key says entity, called unclear
- AdventureWorks2014: table `employeedepartmenthistory`: the key says snapshot, called event
- AdventureWorks2014: table `employeepayhistory`: the key says snapshot, called event
- AdventureWorks2014: table `personcreditcard`: the key says bridge, called entity
- AdventureWorks2014: table `personphone`: the key says entity, called event
- AdventureWorks2014: table `phonenumbertype`: the key says reference, called entity
- AdventureWorks2014: table `product`: the key says entity, called event
- AdventureWorks2014: table `productcategory`: the key says reference, called snapshot
- AdventureWorks2014: table `productdescription`: the key says reference, called entity
- AdventureWorks2014: table `productdocument`: the key says bridge, called event
- AdventureWorks2014: table `productinventory`: the key says bridge, called event
- AdventureWorks2014: table `productmodelillustration`: the key says bridge, called unclear
- AdventureWorks2014: table `productmodelproductdescriptionculture`: the key says bridge, called event
- AdventureWorks2014: table `productproductphoto`: the key says bridge, called event
- AdventureWorks2014: table `productreview`: the key says event, called unclear
- AdventureWorks2014: table `productsubcategory`: the key says reference, called event
- AdventureWorks2014: table `productvendor`: the key says bridge, called event
- AdventureWorks2014: table `salesorderheadersalesreason`: the key says bridge, called unclear
- AdventureWorks2014: table `salesperson`: the key says entity, called event
- AdventureWorks2014: table `salespersonquotahistory`: the key says snapshot, called event
- AdventureWorks2014: table `salesreason`: the key says reference, called entity
- AdventureWorks2014: table `salestaxrate`: the key says entity, called unclear
- AdventureWorks2014: table `salesterritory`: the key says entity, called unclear
- AdventureWorks2014: table `salesterritoryhistory`: the key says snapshot, called event
- AdventureWorks2014: table `scrapreason`: the key says reference, called entity
- AdventureWorks2014: table `shoppingcartitem`: the key says event, called entity
- AdventureWorks2014: table `specialofferproduct`: the key says bridge, called event
- AdventureWorks2014: table `stateprovince`: the key says entity, called event
- AdventureWorks2014: table `store`: the key says entity, called snapshot
- AdventureWorks2014: table `transactionhistoryarchive`: called event, not in the key
- AdventureWorks2014: table `unitmeasure`: the key says reference, called entity
- Basketball_men: table `awards_coaches`: the key says event, called unclear
- Basketball_men: table `coaches`: the key says snapshot, called unclear
- Basketball_men: table `players_teams`: the key says snapshot, called event
- Basketball_men: table `series_post`: the key says event, called unclear
- Basketball_men: table `teams`: the key says snapshot, called unclear
- Chinook: fold `playlist` into `playlisttrack`: called, not in the key
- Chinook: table `employee`: the key says entity, called unclear
- Chinook: table `invoiceline`: the key says event, called entity
- Chinook: table `playlist`: the key says entity, called reference
- Countries: fold `metadata - indicators` into `data`: in the key, not called
- Countries: table `data`: the key says snapshot, called entity
- Countries: table `metadata - indicators`: the key says reference, called unclear
- Countries: table `target`: the key says snapshot, called entity
- ErgastF1: fold `seasons` into `races`: in the key, not called
- ErgastF1: table `constructorresults`: the key says event, called entity
- ErgastF1: table `constructorstandings`: the key says snapshot, called entity
- ErgastF1: table `driverstandings`: the key says snapshot, called entity
- ErgastF1: table `laptimes`: the key says event, called unclear
- ErgastF1: table `qualifying`: the key says event, called entity
- ErgastF1: table `results`: the key says event, called entity
- ErgastF1: table `seasons`: the key says reference, called unclear
- ErgastF1: table `target`: the key says snapshot, called unclear
- GOSales: table `go_products`: the key says entity, called unclear
- Hockey: table `abbrev`: the key says reference, called unclear
- Hockey: table `awardsmisc`: the key says event, called unclear
- Hockey: table `coaches`: the key says snapshot, called event
- Hockey: table `combinedshutouts`: the key says event, called unclear
- Hockey: table `goalies`: the key says snapshot, called event
- Hockey: table `goaliessc`: the key says snapshot, called event
- Hockey: table `goaliesshootout`: the key says snapshot, called event
- Hockey: table `master`: the key says entity, called unclear
- Hockey: table `scoring`: the key says snapshot, called event
- Hockey: table `scoringsc`: the key says snapshot, called event
- Hockey: table `scoringshootout`: the key says snapshot, called event
- Hockey: table `scoringsup`: the key says snapshot, called event
- Hockey: table `seriespost`: the key says event, called unclear
- Hockey: table `teams`: the key says snapshot, called unclear
- Hockey: table `teamshalf`: the key says snapshot, called unclear
- Hockey: table `teamsplits`: the key says snapshot, called unclear
- Hockey: table `teamspost`: the key says snapshot, called unclear
- Hockey: table `teamvsteam`: the key says snapshot, called unclear
- Mondial: table `continent`: the key says entity, called unclear
- Mondial: table `desert`: the key says entity, called unclear
- Mondial: table `economy`: the key says entity, called unclear
- Mondial: table `encompasses`: the key says bridge, called entity
- Mondial: table `ethnicgroup`: the key says bridge, called entity
- Mondial: table `geo_desert`: the key says bridge, called entity
- Mondial: table `geo_estuary`: the key says bridge, called entity
- Mondial: table `geo_island`: the key says bridge, called entity
- Mondial: table `geo_lake`: the key says bridge, called entity
- Mondial: table `geo_mountain`: the key says bridge, called entity
- Mondial: table `geo_river`: the key says bridge, called unclear
- Mondial: table `geo_sea`: the key says bridge, called unclear
- Mondial: table `geo_source`: the key says bridge, called entity
- Mondial: table `island`: the key says entity, called unclear
- Mondial: table `islandin`: the key says bridge, called unclear
- Mondial: table `ismember`: the key says bridge, called event
- Mondial: table `lake`: the key says entity, called unclear
- Mondial: table `language`: the key says bridge, called event
- Mondial: table `located`: the key says bridge, called unclear
- Mondial: table `locatedon`: the key says bridge, called entity
- Mondial: table `mergeswith`: the key says bridge, called unclear
- Mondial: table `mountain`: the key says entity, called unclear
- Mondial: table `mountainonisland`: the key says bridge, called unclear
- Mondial: table `politics`: the key says entity, called unclear
- Mondial: table `population`: the key says entity, called unclear
- Mondial: table `religion`: the key says bridge, called entity
- Mondial: table `river`: the key says entity, called unclear
- Mondial: table `sea`: the key says entity, called unclear
- PremierLeague: fold `players` into `actions`: called, not in the key
- PremierLeague: table `actions`: the key says event, called unclear
- PremierLeague: table `players`: the key says entity, called reference
- PremierLeague: table `teams`: the key says entity, called reference
- TubePricing: fold `tube_end_form` into `tube`: in the key, not called
- TubePricing: table `bill_of_materials`: the key says bridge, called event
- TubePricing: table `spec`: the key says bridge, called unclear
- TubePricing: table `tube_end_form`: the key says reference, called unclear
- TubePricing: table `type_end_form`: the key says reference, called unclear
- Walmart_2014: table `train`: the key says snapshot, called event
- ccs: table `customers`: the key says entity, called unclear
- classicmodels: fold `productlines` into `products`: in the key, not called
- classicmodels: table `orderdetails`: the key says event, called entity
- classicmodels: table `productlines`: the key says reference, called unclear
- employee: table `departments`: the key says reference, called unclear
- employee: table `dept_emp`: the key says snapshot, called event
- employee: table `dept_manager`: the key says snapshot, called entity
- employee: table `titles`: the key says snapshot, called event
- financial: table `account`: the key says entity, called event
- financial: table `disp`: the key says bridge, called entity
- imdb_MovieLens: table `movies2directors`: the key says bridge, called unclear
- imdb_small: table `directors_genres`: the key says bridge, called entity
- imdb_small: table `directors`: the key says entity, called reference
- imdb_small: table `movies_directors`: the key says bridge, called entity
- imdb_small: table `movies_genres`: the key says bridge, called unclear
- imdb_small: table `movies`: the key says entity, called unclear
- imdb_small: table `roles`: the key says bridge, called entity
- lahman_2014: table `appearances`: the key says snapshot, called event
- lahman_2014: table `awardsmanagers`: the key says event, called unclear
- lahman_2014: table `awardssharemanagers`: the key says event, called unclear
- lahman_2014: table `batting`: the key says snapshot, called event
- lahman_2014: table `battingpost`: the key says snapshot, called event
- lahman_2014: table `els_teamnames`: the key says unclear, called entity
- lahman_2014: table `fielding`: the key says snapshot, called event
- lahman_2014: table `fieldingof`: the key says snapshot, called unclear
- lahman_2014: table `fieldingpost`: the key says snapshot, called event
- lahman_2014: table `managers`: the key says snapshot, called unclear
- lahman_2014: table `pitching`: the key says snapshot, called event
- lahman_2014: table `pitchingpost`: the key says snapshot, called event
- lahman_2014: table `salaries`: the key says snapshot, called unclear
- lahman_2014: table `schoolsplayers`: the key says snapshot, called event
- lahman_2014: table `seriespost`: the key says event, called unclear
- lahman_2014: table `teams`: the key says snapshot, called event
- lahman_2014: table `teamshalf`: the key says snapshot, called unclear
- northwind: fold `categories` into `products`: in the key, not called
- northwind: fold `region` into `territories`: in the key, not called
- northwind: table `categories`: the key says reference, called entity
- northwind: table `region`: the key says reference, called entity
- northwind: table `shippers`: the key says entity, called unclear
- pubs: fold `jobs` into `employee`: in the key, not called
- pubs: table `jobs`: the key says reference, called entity
- pubs: table `titles`: the key says entity, called event
- sakila: fold `actor` into `film_actor`: called, not in the key
- sakila: fold `country` into `city`: in the key, not called
- sakila: table `actor`: the key says entity, called reference
- sakila: table `country`: the key says reference, called entity
- sakila: table `customer`: the key says entity, called event
- sakila: table `film_category`: the key says bridge, called entity
- sakila: table `film`: the key says entity, called snapshot
- sakila: table `inventory`: the key says entity, called event
- stats_CEB: table `tags`: the key says entity, called unclear
- world: table `countrylanguage`: the key says bridge, called entity

## Tier 2, join recall

A declared single-column foreign key between two tables is found when `relate` proposes the same child and parent columns as an accepted or suggested edge, in that direction. Self-references are not scored, since `relate` skips same-table pairs by design. A composite key is not scored either, since `relate` proposes single-column joins; the last column counts the composite keys with at least one of their column pairs proposed.

| set | typing | declared | found | accepted | recall | self-references | composite (a pair found) |
|---|---|---|---|---|---|---|---|
| AdventureWorks2014 | floor | 89 | 80 | 80 | 89.9% | 0 | 1 (1) |
| Basketball_men | floor | 3 | 3 | 3 | 100.0% | 0 | 6 (0) |
| Chinook | floor | 10 | 9 | 9 | 90.0% | 1 | 0 (0) |
| Countries | floor | 3 | 2 | 2 | 66.7% | 0 | 0 (0) |
| CraftBeer | floor | 1 | 1 | 1 | 100.0% | 0 | 0 (0) |
| ErgastF1 | floor | 21 | 21 | 21 | 100.0% | 0 | 0 (0) |
| GOSales | floor | 3 | 2 | 2 | 66.7% | 0 | 1 (0) |
| Hockey | floor | 12 | 8 | 8 | 66.7% | 0 | 15 (0) |
| Mondial | floor | 48 | 24 | 24 | 50.0% | 0 | 15 (0) |
| PremierLeague | floor | 5 | 5 | 5 | 100.0% | 0 | 0 (0) |
| TubePricing | floor | 39 | 25 | 25 | 64.1% | 0 | 0 (0) |
| Walmart_2014 | floor | 2 | 0 | 0 | 0.0% | 0 | 0 (0) |
| ccs | floor | 4 | 1 | 1 | 25.0% | 0 | 0 (0) |
| classicmodels | floor | 7 | 5 | 5 | 71.4% | 1 | 0 (0) |
| employee | floor | 6 | 0 | 0 | 0.0% | 0 | 0 (0) |
| financial | floor | 8 | 8 | 8 | 100.0% | 0 | 0 (0) |
| imdb_MovieLens | floor | 6 | 6 | 6 | 100.0% | 0 | 0 (0) |
| imdb_small | floor | 6 | 3 | 3 | 50.0% | 0 | 0 (0) |
| lahman_2014 | floor | 31 | 13 | 2 | 41.9% | 0 | 0 (0) |
| northwind | floor | 12 | 9 | 9 | 75.0% | 1 | 0 (0) |
| pubs | floor | 10 | 10 | 10 | 100.0% | 0 | 0 (0) |
| sakila | floor | 22 | 20 | 20 | 90.9% | 0 | 0 (0) |
| stats_CEB | floor | 12 | 8 | 8 | 66.7% | 0 | 0 (0) |
| university | floor | 4 | 4 | 4 | 100.0% | 0 | 0 (0) |
| world | floor | 2 | 2 | 2 | 100.0% | 0 | 0 (0) |
| AdventureWorks2014 | profiled | 89 | 80 | 80 | 89.9% | 0 | 1 (1) |
| Basketball_men | profiled | 3 | 3 | 3 | 100.0% | 0 | 6 (0) |
| Chinook | profiled | 10 | 9 | 9 | 90.0% | 1 | 0 (0) |
| Countries | profiled | 3 | 2 | 2 | 66.7% | 0 | 0 (0) |
| CraftBeer | profiled | 1 | 1 | 1 | 100.0% | 0 | 0 (0) |
| ErgastF1 | profiled | 21 | 20 | 20 | 95.2% | 0 | 0 (0) |
| GOSales | profiled | 3 | 2 | 2 | 66.7% | 0 | 1 (0) |
| Hockey | profiled | 12 | 8 | 8 | 66.7% | 0 | 15 (0) |
| Mondial | profiled | 48 | 24 | 24 | 50.0% | 0 | 15 (0) |
| PremierLeague | profiled | 5 | 5 | 5 | 100.0% | 0 | 0 (0) |
| TubePricing | profiled | 39 | 25 | 25 | 64.1% | 0 | 0 (0) |
| Walmart_2014 | profiled | 2 | 2 | 2 | 100.0% | 0 | 0 (0) |
| ccs | profiled | 4 | 1 | 1 | 25.0% | 0 | 0 (0) |
| classicmodels | profiled | 7 | 4 | 4 | 57.1% | 1 | 0 (0) |
| employee | profiled | 6 | 4 | 4 | 66.7% | 0 | 0 (0) |
| financial | profiled | 8 | 8 | 8 | 100.0% | 0 | 0 (0) |
| imdb_MovieLens | profiled | 6 | 6 | 6 | 100.0% | 0 | 0 (0) |
| imdb_small | profiled | 6 | 3 | 3 | 50.0% | 0 | 0 (0) |
| lahman_2014 | profiled | 31 | 13 | 2 | 41.9% | 0 | 0 (0) |
| northwind | profiled | 12 | 9 | 9 | 75.0% | 1 | 0 (0) |
| pubs | profiled | 10 | 10 | 10 | 100.0% | 0 | 0 (0) |
| sakila | profiled | 22 | 20 | 20 | 90.9% | 0 | 0 (0) |
| stats_CEB | profiled | 12 | 8 | 8 | 66.7% | 0 | 0 (0) |
| university | profiled | 4 | 4 | 4 | 100.0% | 0 | 0 (0) |
| world | profiled | 2 | 2 | 2 | 100.0% | 0 | 0 (0) |

Missed foreign keys, floor:

- AdventureWorks2014: `billofmaterials.componentid` -> `product.productid`
- AdventureWorks2014: `currencyrate.fromcurrencycode` -> `currency.currencycode`
- AdventureWorks2014: `currencyrate.tocurrencycode` -> `currency.currencycode`
- AdventureWorks2014: `document.owner` -> `employee.businessentityid`
- AdventureWorks2014: `product.sizeunitmeasurecode` -> `unitmeasure.unitmeasurecode`
- AdventureWorks2014: `product.weightunitmeasurecode` -> `unitmeasure.unitmeasurecode`
- AdventureWorks2014: `productdocument.documentnode` -> `document.documentnode`
- AdventureWorks2014: `salesorderheader.billtoaddressid` -> `address.addressid`
- AdventureWorks2014: `salesorderheader.shiptoaddressid` -> `address.addressid`
- Chinook: `customer.supportrepid` -> `employee.employeeid`
- Countries: `data.indicator code` -> `metadata - indicators.indicator_code`
- GOSales: `go_1k.product number` -> `go_products.product number`
- Hockey: `awardscoaches.coachid` -> `coaches.coachid`
- Hockey: `combinedshutouts.idgoalie1` -> `master.playerid`
- Hockey: `combinedshutouts.idgoalie2` -> `master.playerid`
- Hockey: `master.coachid` -> `coaches.coachid`
- Mondial: `encompasses.continent` -> `continent.name`
- Mondial: `geo_desert.desert` -> `desert.name`
- Mondial: `geo_estuary.river` -> `river.name`
- Mondial: `geo_island.island` -> `island.name`
- Mondial: `geo_lake.lake` -> `lake.name`
- Mondial: `geo_mountain.mountain` -> `mountain.name`
- Mondial: `geo_river.river` -> `river.name`
- Mondial: `geo_sea.sea` -> `sea.name`
- Mondial: `geo_source.river` -> `river.name`
- Mondial: `islandin.island` -> `island.name`
- Mondial: `islandin.lake` -> `lake.name`
- Mondial: `islandin.river` -> `river.name`
- Mondial: `islandin.sea` -> `sea.name`
- Mondial: `ismember.organization` -> `organization.abbreviation`
- Mondial: `located.lake` -> `lake.name`
- Mondial: `located.river` -> `river.name`
- Mondial: `located.sea` -> `sea.name`
- Mondial: `locatedon.island` -> `island.name`
- Mondial: `mergeswith.sea1` -> `sea.name`
- Mondial: `mergeswith.sea2` -> `sea.name`
- Mondial: `mountainonisland.island` -> `island.name`
- Mondial: `mountainonisland.mountain` -> `mountain.name`
- Mondial: `politics.dependent` -> `country.code`
- Mondial: `river.lake` -> `lake.name`
- TubePricing: `comp_adaptor.connection_type_id_1` -> `type_connection.connection_type_id`
- TubePricing: `comp_adaptor.connection_type_id_2` -> `type_connection.connection_type_id`
- TubePricing: `comp_adaptor.end_form_id_1` -> `type_end_form.end_form_id`
- TubePricing: `comp_adaptor.end_form_id_2` -> `type_end_form.end_form_id`
- TubePricing: `comp_threaded.connection_type_id_1` -> `type_connection.connection_type_id`
- TubePricing: `comp_threaded.connection_type_id_2` -> `type_connection.connection_type_id`
- TubePricing: `comp_threaded.connection_type_id_3` -> `type_connection.connection_type_id`
- TubePricing: `comp_threaded.connection_type_id_4` -> `type_connection.connection_type_id`
- TubePricing: `comp_threaded.end_form_id_1` -> `type_end_form.end_form_id`
- TubePricing: `comp_threaded.end_form_id_2` -> `type_end_form.end_form_id`
- TubePricing: `comp_threaded.end_form_id_3` -> `type_end_form.end_form_id`
- TubePricing: `comp_threaded.end_form_id_4` -> `type_end_form.end_form_id`
- TubePricing: `tube.end_a` -> `tube_end_form.end_form_id`
- TubePricing: `tube.end_x` -> `tube_end_form.end_form_id`
- Walmart_2014: `features.store` -> `stores.store`
- Walmart_2014: `train.store` -> `stores.store`
- ccs: `transactions.customerid` -> `customers.customerid`
- ccs: `transactions.gasstationid` -> `gasstations.gasstationid`
- ccs: `transactions.productid` -> `products.productid`
- classicmodels: `customers.salesrepemployeenumber` -> `employees.employeenumber`
- classicmodels: `products.productline` -> `productlines.productline`
- employee: `dept_emp.dept_no` -> `departments.dept_no`
- employee: `dept_emp.emp_no` -> `employees.emp_no`
- employee: `dept_manager.dept_no` -> `departments.dept_no`
- employee: `dept_manager.emp_no` -> `employees.emp_no`
- employee: `salaries.emp_no` -> `employees.emp_no`
- employee: `titles.emp_no` -> `employees.emp_no`
- imdb_small: `movies_directors.movie_id` -> `movies.id`
- imdb_small: `movies_genres.movie_id` -> `movies.id`
- imdb_small: `roles.movie_id` -> `movies.id`
- lahman_2014: `allstarfull.teamid` -> `teams.teamid`
- lahman_2014: `awardsmanagers.managerid` -> `managers.managerid`
- lahman_2014: `awardssharemanagers.managerid` -> `managers.managerid`
- lahman_2014: `batting.teamid` -> `teams.teamid`
- lahman_2014: `battingpost.teamid` -> `teams.teamid`
- lahman_2014: `els_teamnames.teamid` -> `teams.teamid`
- lahman_2014: `fielding.teamid` -> `teams.teamid`
- lahman_2014: `fieldingpost.teamid` -> `teams.teamid`
- lahman_2014: `managers.teamid` -> `teams.teamid`
- lahman_2014: `managershalf.managerid` -> `managers.managerid`
- lahman_2014: `managershalf.teamid` -> `teams.teamid`
- lahman_2014: `pitching.teamid` -> `teams.teamid`
- lahman_2014: `pitchingpost.teamid` -> `teams.teamid`
- lahman_2014: `players.hofid` -> `halloffame.hofid`
- lahman_2014: `salaries.teamid` -> `teams.teamid`
- lahman_2014: `seriespost.teamidloser` -> `teams.teamid`
- lahman_2014: `seriespost.teamidwinner` -> `teams.teamid`
- lahman_2014: `teamshalf.teamid` -> `teams.teamid`
- northwind: `customercustomerdemo.customerid` -> `customers.customerid`
- northwind: `customercustomerdemo.customertypeid` -> `customerdemographics.customertypeid`
- northwind: `orders.shipvia` -> `shippers.shipperid`
- sakila: `film.original_language_id` -> `language.language_id`
- sakila: `store.manager_staff_id` -> `staff.staff_id`
- stats_CEB: `postlinks.relatedpostid` -> `posts.id`
- stats_CEB: `posts.lasteditoruserid` -> `users.id`
- stats_CEB: `posts.owneruserid` -> `users.id`
- stats_CEB: `tags.excerptpostid` -> `posts.id`

Missed foreign keys, profiled:

- AdventureWorks2014: `billofmaterials.componentid` -> `product.productid`
- AdventureWorks2014: `currencyrate.fromcurrencycode` -> `currency.currencycode`
- AdventureWorks2014: `currencyrate.tocurrencycode` -> `currency.currencycode`
- AdventureWorks2014: `document.owner` -> `employee.businessentityid`
- AdventureWorks2014: `product.sizeunitmeasurecode` -> `unitmeasure.unitmeasurecode`
- AdventureWorks2014: `product.weightunitmeasurecode` -> `unitmeasure.unitmeasurecode`
- AdventureWorks2014: `productdocument.documentnode` -> `document.documentnode`
- AdventureWorks2014: `salesorderheader.billtoaddressid` -> `address.addressid`
- AdventureWorks2014: `salesorderheader.shiptoaddressid` -> `address.addressid`
- Chinook: `customer.supportrepid` -> `employee.employeeid`
- Countries: `data.indicator code` -> `metadata - indicators.indicator_code`
- ErgastF1: `races.year` -> `seasons.year`
- GOSales: `go_1k.product number` -> `go_products.product number`
- Hockey: `awardscoaches.coachid` -> `coaches.coachid`
- Hockey: `combinedshutouts.idgoalie1` -> `master.playerid`
- Hockey: `combinedshutouts.idgoalie2` -> `master.playerid`
- Hockey: `master.coachid` -> `coaches.coachid`
- Mondial: `encompasses.continent` -> `continent.name`
- Mondial: `geo_desert.desert` -> `desert.name`
- Mondial: `geo_estuary.river` -> `river.name`
- Mondial: `geo_island.island` -> `island.name`
- Mondial: `geo_lake.lake` -> `lake.name`
- Mondial: `geo_mountain.mountain` -> `mountain.name`
- Mondial: `geo_river.river` -> `river.name`
- Mondial: `geo_sea.sea` -> `sea.name`
- Mondial: `geo_source.river` -> `river.name`
- Mondial: `islandin.island` -> `island.name`
- Mondial: `islandin.lake` -> `lake.name`
- Mondial: `islandin.river` -> `river.name`
- Mondial: `islandin.sea` -> `sea.name`
- Mondial: `ismember.organization` -> `organization.abbreviation`
- Mondial: `located.lake` -> `lake.name`
- Mondial: `located.river` -> `river.name`
- Mondial: `located.sea` -> `sea.name`
- Mondial: `locatedon.island` -> `island.name`
- Mondial: `mergeswith.sea1` -> `sea.name`
- Mondial: `mergeswith.sea2` -> `sea.name`
- Mondial: `mountainonisland.island` -> `island.name`
- Mondial: `mountainonisland.mountain` -> `mountain.name`
- Mondial: `politics.dependent` -> `country.code`
- Mondial: `river.lake` -> `lake.name`
- TubePricing: `comp_adaptor.connection_type_id_1` -> `type_connection.connection_type_id`
- TubePricing: `comp_adaptor.connection_type_id_2` -> `type_connection.connection_type_id`
- TubePricing: `comp_adaptor.end_form_id_1` -> `type_end_form.end_form_id`
- TubePricing: `comp_adaptor.end_form_id_2` -> `type_end_form.end_form_id`
- TubePricing: `comp_threaded.connection_type_id_1` -> `type_connection.connection_type_id`
- TubePricing: `comp_threaded.connection_type_id_2` -> `type_connection.connection_type_id`
- TubePricing: `comp_threaded.connection_type_id_3` -> `type_connection.connection_type_id`
- TubePricing: `comp_threaded.connection_type_id_4` -> `type_connection.connection_type_id`
- TubePricing: `comp_threaded.end_form_id_1` -> `type_end_form.end_form_id`
- TubePricing: `comp_threaded.end_form_id_2` -> `type_end_form.end_form_id`
- TubePricing: `comp_threaded.end_form_id_3` -> `type_end_form.end_form_id`
- TubePricing: `comp_threaded.end_form_id_4` -> `type_end_form.end_form_id`
- TubePricing: `tube.end_a` -> `tube_end_form.end_form_id`
- TubePricing: `tube.end_x` -> `tube_end_form.end_form_id`
- ccs: `transactions.customerid` -> `customers.customerid`
- ccs: `transactions.gasstationid` -> `gasstations.gasstationid`
- ccs: `transactions.productid` -> `products.productid`
- classicmodels: `customers.salesrepemployeenumber` -> `employees.employeenumber`
- classicmodels: `orderdetails.ordernumber` -> `orders.ordernumber`
- classicmodels: `products.productline` -> `productlines.productline`
- employee: `dept_emp.dept_no` -> `departments.dept_no`
- employee: `dept_manager.dept_no` -> `departments.dept_no`
- imdb_small: `movies_directors.movie_id` -> `movies.id`
- imdb_small: `movies_genres.movie_id` -> `movies.id`
- imdb_small: `roles.movie_id` -> `movies.id`
- lahman_2014: `allstarfull.teamid` -> `teams.teamid`
- lahman_2014: `awardsmanagers.managerid` -> `managers.managerid`
- lahman_2014: `awardssharemanagers.managerid` -> `managers.managerid`
- lahman_2014: `batting.teamid` -> `teams.teamid`
- lahman_2014: `battingpost.teamid` -> `teams.teamid`
- lahman_2014: `els_teamnames.teamid` -> `teams.teamid`
- lahman_2014: `fielding.teamid` -> `teams.teamid`
- lahman_2014: `fieldingpost.teamid` -> `teams.teamid`
- lahman_2014: `managers.teamid` -> `teams.teamid`
- lahman_2014: `managershalf.managerid` -> `managers.managerid`
- lahman_2014: `managershalf.teamid` -> `teams.teamid`
- lahman_2014: `pitching.teamid` -> `teams.teamid`
- lahman_2014: `pitchingpost.teamid` -> `teams.teamid`
- lahman_2014: `players.hofid` -> `halloffame.hofid`
- lahman_2014: `salaries.teamid` -> `teams.teamid`
- lahman_2014: `seriespost.teamidloser` -> `teams.teamid`
- lahman_2014: `seriespost.teamidwinner` -> `teams.teamid`
- lahman_2014: `teamshalf.teamid` -> `teams.teamid`
- northwind: `customercustomerdemo.customerid` -> `customers.customerid`
- northwind: `customercustomerdemo.customertypeid` -> `customerdemographics.customertypeid`
- northwind: `orders.shipvia` -> `shippers.shipperid`
- sakila: `film.original_language_id` -> `language.language_id`
- sakila: `store.manager_staff_id` -> `staff.staff_id`
- stats_CEB: `postlinks.relatedpostid` -> `posts.id`
- stats_CEB: `posts.lasteditoruserid` -> `users.id`
- stats_CEB: `posts.owneruserid` -> `users.id`
- stats_CEB: `tags.excerptpostid` -> `posts.id`

## Tier 3, floor

1999 of 2000 sampled tables scored, each alone. Unclear rate 99.0%; reference rate 0.0%.

| role | tables | share |
|---|---|---|
| entity | 18 | 0.9% |
| event | 0 | 0.0% |
| reference | 0 | 0.0% |
| bridge | 0 | 0.0% |
| snapshot | 1 | 0.1% |
| unclear | 1980 | 99.0% |

## Tier 3, profiled

1999 of 2000 sampled tables scored, each alone. Unclear rate 98.4%; reference rate 0.0%.

| role | tables | share |
|---|---|---|
| entity | 10 | 0.5% |
| event | 0 | 0.0% |
| reference | 0 | 0.0% |
| bridge | 0 | 0.0% |
| snapshot | 21 | 1.1% |
| unclear | 1968 | 98.4% |

## Skipped

Tables and databases that could not be loaded or scored, and why.

- gittables/return_on_invested_capital/HNP.parquet (floor): 1726 columns, more than the 1000 a lone table is scored with
- gittables/return_on_invested_capital/HNP.parquet (profiled): 1726 columns, more than the 1000 a lone table is scored with
