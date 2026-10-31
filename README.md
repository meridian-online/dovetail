# dovetail

The modelling layer of Meridian. dovetail discovers how to load unfamiliar data and how it relates, then compiles those decisions into runnable, auditable artifacts. It never executes a pipeline itself.

`dovetail relate` finds the joins between a database's tables and says what each table is for: an entity, an event, a lookup, a link between two others, a snapshot over time, or unclear, each with the evidence behind it. [docs/table-roles.md](docs/table-roles.md) describes how.

`dovetail model` writes an Apache Ossie model of those tables, from a quick sample first and then verified on every row, carrying each role and each join's grade and evidence. [docs/ossie-model.md](docs/ossie-model.md) describes it.

Part of the [Meridian](https://meridian.online) project.
