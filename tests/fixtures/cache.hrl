%% Cache definitions header file
-record(cache_entry, {
    key :: term(),
    val :: term(),
    ttl = 3600 :: integer()
}).

-define(DEFAULT_TTL, 3600).
-define(MAX_CACHE_SIZE, 10000).
-type cache_key() :: binary() | atom().
