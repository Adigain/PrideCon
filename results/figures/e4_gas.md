E4: receipts' gasUsed on anvil (Prague). Source: seed1.csv.

| contract | operation | n | k | gas used | model (precompiles) | > 2^24 |
|---|---|---|---|---|---|---|
| PrideConcurrent | transact | 1 | 0 | 1,289,058 | 449,768 |  |
| PrideConcurrent | transact | 2 | 0 | 1,937,241 | 580,232 |  |
| PrideConcurrent | transact | 4 | 0 | 3,237,790 | 857,072 |  |
| PrideConcurrent | transact | 8 | 0 | 5,854,052 | 1,381,952 |  |
| PrideConcurrent | transact | 16 | 0 | 11,145,159 | 2,411,456 |  |
| PrideConcurrent | transact | 32 | 0 | 21,967,051 | 4,414,304 | yes |
| PrideConcurrent | transact_warm | 1 | 0 | 1,026,716 | 449,768 |  |
| PrideConcurrent | transact_warm | 2 | 0 | 1,412,541 | 580,232 |  |
| PrideConcurrent | transact_warm | 4 | 0 | 2,188,192 | 857,072 |  |
| PrideConcurrent | transact_warm | 8 | 0 | 3,755,296 | 1,381,952 |  |
| PrideConcurrent | transact_warm | 16 | 0 | 6,947,599 | 2,411,456 |  |
| PrideConcurrent | transact_warm | 32 | 0 | 13,572,337 | 4,414,304 |  |
| PrideConcurrent | spend | 1 | 0 | 1,390,028 | 449,768 |  |
| PrideConcurrent | compact | 0 | 0 | 208,574 | 70,300 |  |
| PrideConcurrent | spend | 1 | 1 | 1,397,013 | 449,768 |  |
| PrideConcurrent | compact | 0 | 1 | 257,382 | 70,300 |  |
| PrideConcurrent | spend | 1 | 2 | 1,710,940 | 691,670 |  |
| PrideConcurrent | compact | 0 | 2 | 572,994 | 312,202 |  |
| PrideConcurrent | spend | 1 | 4 | 2,265,801 | 1,075,004 |  |
| PrideConcurrent | compact | 0 | 4 | 1,123,166 | 695,536 |  |
| PrideConcurrent | spend | 1 | 8 | 3,938,308 | 2,231,600 |  |
| PrideConcurrent | compact | 0 | 8 | 2,780,104 | 1,852,132 |  |
| PrideConcurrent | spend | 1 | 16 | 9,764,986 | 6,004,088 |  |
| PrideConcurrent | compact | 0 | 16 | 8,551,442 | 5,624,620 |  |
| PrideConcurrent | spend | 1 | 32 | 33,852,887 | 18,878,984 | yes |
| PrideConcurrent | compact | 0 | 32 | 32,429,804 | 18,499,516 | yes |
| PriDeFULBaseline | transact | 1 | 0 | 183,219 |  |  |
| PriDeFULBaseline | transact | 2 | 0 | 282,399 |  |  |
| PriDeFULBaseline | transact | 4 | 0 | 481,361 |  |  |
| PriDeFULBaseline | transact | 8 | 0 | 881,991 |  |  |
| PriDeFULBaseline | transact | 16 | 0 | 1,694,540 |  |  |
| PriDeFULBaseline | transact | 32 | 0 | 3,363,362 |  |  |
| PrideConcurrent | register (median) |  |  | 502,302 |  |  |
| PriDeFULBaseline | register (median) |  |  | 302,232 |  |  |
| PrideConcurrent | mint (median) |  |  | 90,483 |  |  |
| PriDeFULBaseline | mint (median) |  |  | 86,252 |  |  |
