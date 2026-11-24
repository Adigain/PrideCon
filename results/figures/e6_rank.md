Generic-group rank test (ggm.py rows report GT only). Source: seed1-exp100000-bls2000-rank.csv.

| game | case | ranks real (G1, G2, GT) | ranks random | leak | expected |
|---|---|---|---|---|---|
| ggm | original PriDeFUL (single tx) | (0, 0, 9) | (0, 0, 9) | false | false |
| ggm | original, prior balance b known | (0, 0, 8) | (0, 0, 9) | true | true |
| ggm | plan Method A (raw g2^delta) | (0, 0, 36) | (0, 0, 37) | true | true |
| ggm | tagged handles, no epoch binding, x0=0 | (0, 0, 236) | (0, 0, 237) | true | true |
| ggm | tagged handles, no epoch binding | (0, 0, 248) | (0, 0, 248) | false | false |
| ggm | tagged handles + epoch binding | (0, 0, 409) | (0, 0, 409) | false | false |
| ggm | tagged + epoch binding, x0=0 | (0, 0, 394) | (0, 0, 394) | false | false |
| ggm | tagged + epoch binding, tx* alone | (0, 0, 198) | (0, 0, 198) | false | false |
| ggm | tagged + epoch binding, b known | (0, 0, 409) | (0, 0, 409) | false | false |
| tdddh | TD-DDH, hidden uniform balance (variant i) | (19, 8, 125) | (19, 8, 125) | false | false |
| tdddh | TD-DDH, known total, equal sums (variant ii) | (18, 8, 117) | (18, 8, 117) | false | false |
| tdddh | TD-DDH without prior-epoch x-handles | (17, 8, 112) | (17, 8, 112) | false | false |
| tdddh | control: x-handles in SAME epoch | (19, 8, 124) | (19, 8, 125) | true | true |
| tdddh | control: raw g2^delta | (19, 6, 82) | (19, 6, 84) | true | true |
| tdddh | control: E split into x*d and r*d | (19, 8, 125) | (20, 8, 133) | true | true |
| tdddh | control: epoch-start randomness x known | (15, 8, 96) | (16, 8, 104) | true | true |
