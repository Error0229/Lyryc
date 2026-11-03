| rule | rejected | precision (>1 s) | precision (>0.5 s) | recall (>1 s) | kept ≤0.3 s | kept ≤1.0 s |
|---|---|---|---|---|---|---|
| none | 0% | – | – | – | 79% | 89% |
| conf<0.10 | 16% | 40% | 51% | 58% | 86% | 94% |
| smoothed3 conf<0.10 | 16% | 37% | 49% | 52% | 85% | 94% |
| smoothed3 conf<0.12 | 18% | 37% | 47% | 60% | 86% | 95% |
| conf<0.4×track median | 14% | 36% | 46% | 43% | 84% | 93% |
| track: >60% lines conf<0.15 | 8% | 47% | 60% | 35% | 83% | 92% |
| smoothed3<0.10 OR track >60% low | 20% | 36% | 47% | 62% | 87% | 95% |
| [LRCLIB subset] conf<0.10 | 12% | 43% | 54% | 52% | 87% | 94% |
| [LRCLIB subset] off LRCLIB line by >0.7 s | 11% | 79% | 87% | 86% | 91% | 98% |
| [LRCLIB subset] conf<0.10 OR off LRCLIB >0.7 s | 18% | 53% | 62% | 91% | 92% | 99% |
