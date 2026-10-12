-module(cache_server).
-behaviour(gen_server).

-include("cache.hrl").

%% API exports
-export([start_link/0, get/1, put/2]).

%% gen_server callbacks
-export([init/1, handle_call/3, handle_cast/2, handle_info/2, terminate/2, code_change/3]).

-record(state, {
    entries = #{} :: map(),
    size = 0 :: integer()
}).

start_link() ->
    gen_server:start_link({local, ?MODULE}, ?MODULE, [], []).

get(Key) ->
    gen_server:call(?MODULE, {get, Key}).

put(Key, Val) ->
    gen_server:call(?MODULE, {put, Key, Val}).

init([]) ->
    {ok, #state{}}.

handle_call({get, Key}, _From, #state{entries = Entries} = State) ->
    Reply = maps:find(Key, Entries),
    {reply, Reply, State};
handle_call({put, Key, Val}, _From, #state{entries = Entries, size = Size} = State) ->
    NewEntries = maps:put(Key, Val, Entries),
    {reply, ok, State#state{entries = NewEntries, size = Size + 1}}.

handle_cast(_Msg, State) ->
    {noreply, State}.

handle_info(_Info, State) ->
    {noreply, State}.

terminate(_Reason, _State) ->
    ok.

code_change(_OldVsn, State, _Extra) ->
    {ok, State}.
