# jtd 저장 실험 (2026-10-01)

一太郎(또는 一太郎 뷰어)에서 각 파일을 열어 결과를 적는다.

| 파일 | 바꾼 것 | 묻는 것 |
|---|---|---|
| 00-original | 없음(원본) | 기준 |
| 01-repack | 내용 그대로, 우리 CFB 쓰기로 다시 포장 | 우리 껍데기(CFB)를 받아 주나 |
| 02-textv-repack | 본문 칸(SsmgV.01)을 우리 방식으로 다시 포장 | 본문 저장소 형식이 맞나 |
| 03-replace-1char | 첫 글자 하나를 '試'로 (길이 같음) | 검사합(체크섬)이 있나 |
| 04-insert-keep-caches | 「【EZPZ編集テスト】」 삽입, 위치 표는 그대로(어긋남) | 위치 표가 어긋나도 다시 계산하나 |
| 05-insert-no-marks | 04 + LineMark·PageMark 삭제 | 줄·쪽 표가 없어도 여나 |
| 06-insert-no-caches | 05 + DocumentTextPositionTables 삭제 | 위치 표가 전부 없어도 여나 |
| 07-nochange-no-marks | 글자 변경 없이 LineMark·PageMark만 삭제 | 줄·쪽 표가 필수인가 |

A = 1258443_001 (양식, 표 많음, 위치 표 없음) / B = 1258443_049 (DocumentTextPositionTables 있음)

결과 기록: `results.md`
