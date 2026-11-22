Empty directory marker: the CLI's own migration/seed passes are disabled in
config.toml (the api suite applies the generated migrations itself via psql,
into the [database.api] target).
