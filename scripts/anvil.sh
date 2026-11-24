#!/usr/bin/env bash
# Local chain for the experiments (Prague hardfork, SPEC S6).
exec anvil --hardfork prague --gas-limit 100000000 --code-size-limit 100000 --block-time 2
