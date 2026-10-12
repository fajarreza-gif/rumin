#!/usr/bin/env escript
%% -*- erlang -*-
%%! -sname cache_ctl

main(["start"]) ->
    io:format("Starting cache_app...~n");
main(["stop"]) ->
    io:format("Stopping cache_app...~n");
main(_) ->
    io:format("Usage: cache_ctl start|stop~n"),
    halt(1).
