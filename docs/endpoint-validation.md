# Live endpoint validation

Checked 2026-11-11T18:53:38.902247+00:00 through the actual CLI, without credentials. Racing date: 2026-11-11.

| Result | Routes |
| --- | ---: |
| success_nonempty | 57 |
| success_empty | 3 |

- One representative request per route; filter combinations and returned field accuracy are not exhaustively verified.
- Empty success establishes access and decoding, not correct filtering or useful data.
- Aliases share canonical routes and are covered separately by the offline dispatch test.

| Tool | Result | HTTP | Note |
| --- | --- | --- | --- |
| `equidia_get_broadcast_meetings_by_date_editorial` | success_nonempty | 200 |  |
| `equidia_get_broadcast_meetings_by_guid_editorial` | success_nonempty | 200 |  |
| `equidia_get_broadcast_races_by_guid_editorial` | success_nonempty | 200 |  |
| `equidia_get_broadcast_tickets_by_guid` | success_empty | 200 |  |
| `equidia_get_desk_race_clipping` | success_nonempty | 200 |  |
| `equidia_get_hexaglobe_live_streams` | success_nonempty | 200 |  |
| `equidia_get_mobile_daily_predictions` | success_nonempty | 200 |  |
| `equidia_get_mobile_horse` | success_nonempty | 200 |  |
| `equidia_get_mobile_horse_stats` | success_nonempty | 200 |  |
| `equidia_get_mobile_last_programmings` | success_nonempty | 200 |  |
| `equidia_get_mobile_live_streams` | success_nonempty | 200 |  |
| `equidia_get_mobile_meetings` | success_nonempty | 200 |  |
| `equidia_get_mobile_race` | success_nonempty | 200 |  |
| `equidia_get_mobile_race_quinte` | success_nonempty | 200 |  |
| `equidia_get_mobile_races` | success_nonempty | 200 |  |
| `equidia_get_mobile_races_by_family_discipline` | success_nonempty | 200 |  |
| `equidia_get_mobile_races_by_highlight` | success_nonempty | 200 |  |
| `equidia_get_mobile_races_by_meeting` | success_nonempty | 200 |  |
| `equidia_get_mobile_races_by_meeting_number` | success_nonempty | 200 |  |
| `equidia_get_mobile_races_pulling` | success_nonempty | 200 |  |
| `equidia_get_mobile_runner` | success_nonempty | 200 |  |
| `equidia_get_mobile_runner_by_trainer_or_rider` | success_nonempty | 200 |  |
| `equidia_get_mobile_runners` | success_nonempty | 200 |  |
| `equidia_get_mobile_runners_analyse` | success_nonempty | 200 |  |
| `equidia_get_mobile_runners_by_horse` | success_nonempty | 200 |  |
| `equidia_get_mobile_social_prono` | success_nonempty | 200 |  |
| `equidia_get_ordonnanceur_batch_favorite_runner` | success_nonempty | 200 |  |
| `equidia_get_pro_live_streams` | success_nonempty | 200 |  |
| `equidia_get_tv_epg` | success_nonempty | 200 |  |
| `equidia_get_tv_last_episodes` | success_nonempty | 200 |  |
| `equidia_get_tv_last_quinte` | success_nonempty | 200 |  |
| `equidia_get_tv_live_streams` | success_nonempty | 200 |  |
| `equidia_get_tv_meetings` | success_nonempty | 200 |  |
| `equidia_get_tv_race` | success_nonempty | 200 |  |
| `equidia_get_tv_races_by_meeting` | success_nonempty | 200 |  |
| `equidia_get_tv_shows` | success_nonempty | 200 |  |
| `equidia_get_v2_articles` | success_nonempty | 200 |  |
| `equidia_get_v2_categories` | success_nonempty | 200 |  |
| `equidia_get_v2_editorial_contents` | success_nonempty | 200 |  |
| `equidia_get_v2_image_parts` | success_nonempty | 200 |  |
| `equidia_get_v2_image_parts_by_id` | success_nonempty | 200 |  |
| `equidia_get_v2_medias` | success_nonempty | 200 |  |
| `equidia_get_v2_meetings` | success_nonempty | 200 |  |
| `equidia_get_v2_meetings_by_guid` | success_nonempty | 200 |  |
| `equidia_get_v2_optiweb` | success_nonempty | 200 |  |
| `equidia_get_v2_pre_runners` | success_empty | 200 |  |
| `equidia_get_v2_rundowns` | success_nonempty | 200 |  |
| `equidia_get_v2_rundowns_by_uuid` | success_nonempty | 200 |  |
| `equidia_get_v2_runners` | success_nonempty | 200 |  |
| `equidia_get_v2_runners_by_uuid` | success_nonempty | 200 |  |
| `equidia_get_v2_segments` | success_nonempty | 200 |  |
| `equidia_get_v2_segments_by_id` | success_nonempty | 200 |  |
| `equidia_get_v2_stable_noise` | success_empty | 200 |  |
| `equidia_get_v2_subscriptions` | success_nonempty | 200 |  |
| `equidia_get_v2_subscriptions_by_id` | success_nonempty | 200 |  |
| `equidia_get_v2_tags` | success_nonempty | 200 |  |
| `equidia_get_v2_user_optiwebs` | success_nonempty | 200 |  |
| `equidia_get_v2_users` | success_nonempty | 200 |  |
| `equidia_get_v2_users_by_uuid` | success_nonempty | 200 |  |
| `equidia_get_web_article_mobile` | success_nonempty | 200 |  |
