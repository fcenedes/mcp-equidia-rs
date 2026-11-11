# Read tool reference

Generated from the bundled Equidia OpenAPI snapshot (2026-11-11). All canonical tools use GET.

| Tool | Endpoint | Required arguments |
| --- | --- | --- |
| `equidia_get_broadcast_meetings_by_date_editorial` | `/api/broadcast/meetings/{date}/editorial` | `date` |
| `equidia_get_broadcast_meetings_by_guid_editorial` | `/api/broadcast/meetings/{guid}/editorial` | `guid` |
| `equidia_get_broadcast_races_by_guid_editorial` | `/api/broadcast/races/{guid}/editorial` | `guid` |
| `equidia_get_broadcast_tickets_by_guid` | `/api/broadcast/tickets/{guid}` | `guid` |
| `equidia_get_desk_race_clipping` | `/api/desk/race-clipping` | — |
| `equidia_get_hexaglobe_live_streams` | `/api/hexaglobe/live-streams` | — |
| `equidia_get_mobile_daily_predictions` | `/api/mobile/daily-predictions` | — |
| `equidia_get_mobile_horse` | `/api/mobile/horse` | — |
| `equidia_get_mobile_horse_stats` | `/api/mobile/horse-stats` | — |
| `equidia_get_mobile_last_programmings` | `/api/mobile/last-programmings` | — |
| `equidia_get_mobile_live_streams` | `/api/mobile/live-streams` | — |
| `equidia_get_mobile_meetings` | `/api/mobile/meetings` | — |
| `equidia_get_mobile_race` | `/api/mobile/race` | — |
| `equidia_get_mobile_race_quinte` | `/api/mobile/race-quinte` | — |
| `equidia_get_mobile_races` | `/api/mobile/races` | — |
| `equidia_get_mobile_races_by_family_discipline` | `/api/mobile/races-by-family-discipline` | — |
| `equidia_get_mobile_races_by_highlight` | `/api/mobile/races-by-highlight` | — |
| `equidia_get_mobile_races_by_meeting` | `/api/mobile/races-by-meeting` | — |
| `equidia_get_mobile_races_by_meeting_number` | `/api/mobile/races-by-meeting-number` | — |
| `equidia_get_mobile_races_pulling` | `/api/mobile/races-pulling` | — |
| `equidia_get_mobile_runner` | `/api/mobile/runner` | — |
| `equidia_get_mobile_runner_by_trainer_or_rider` | `/api/mobile/runner-by-trainer-or-rider` | — |
| `equidia_get_mobile_runners` | `/api/mobile/runners` | — |
| `equidia_get_mobile_runners_analyse` | `/api/mobile/runners-analyse` | — |
| `equidia_get_mobile_runners_by_horse` | `/api/mobile/runners-by-horse` | `horse-uuid` |
| `equidia_get_mobile_social_prono` | `/api/mobile/social-prono` | — |
| `equidia_get_ordonnanceur_batch_favorite_runner` | `/api/ordonnanceur/batch-favorite-runner` | — |
| `equidia_get_pro_live_streams` | `/api/pro/live-streams` | — |
| `equidia_get_tv_epg` | `/api/tv/epg` | — |
| `equidia_get_tv_last_episodes` | `/api/tv/last-episodes` | — |
| `equidia_get_tv_last_quinte` | `/api/tv/last-quinte` | — |
| `equidia_get_tv_live_streams` | `/api/tv/live-streams` | — |
| `equidia_get_tv_meetings` | `/api/tv/meetings` | — |
| `equidia_get_tv_race` | `/api/tv/race` | — |
| `equidia_get_tv_races_by_meeting` | `/api/tv/races-by-meeting` | — |
| `equidia_get_tv_shows` | `/api/tv/shows` | — |
| `equidia_get_v2_articles` | `/api/v2/articles` | — |
| `equidia_get_v2_categories` | `/api/v2/categories` | — |
| `equidia_get_v2_editorial_contents` | `/api/v2/editorial-contents` | — |
| `equidia_get_v2_image_parts` | `/api/v2/image-parts` | — |
| `equidia_get_v2_image_parts_by_id` | `/api/v2/image-parts/{id}` | `id` |
| `equidia_get_v2_medias` | `/api/v2/medias` | — |
| `equidia_get_v2_meetings` | `/api/v2/meetings` | — |
| `equidia_get_v2_meetings_by_guid` | `/api/v2/meetings/{guid}` | `guid` |
| `equidia_get_v2_optiweb` | `/api/v2/optiweb` | `start`, `end` |
| `equidia_get_v2_pre_runners` | `/api/v2/pre-runners` | — |
| `equidia_get_v2_rundowns` | `/api/v2/rundowns` | — |
| `equidia_get_v2_rundowns_by_uuid` | `/api/v2/rundowns/{uuid}` | `uuid` |
| `equidia_get_v2_runners` | `/api/v2/runners` | — |
| `equidia_get_v2_runners_by_uuid` | `/api/v2/runners/{uuid}` | `uuid` |
| `equidia_get_v2_segments` | `/api/v2/segments` | — |
| `equidia_get_v2_segments_by_id` | `/api/v2/segments/{id}` | `id` |
| `equidia_get_v2_stable_noise` | `/api/v2/stable-noise` | — |
| `equidia_get_v2_subscriptions` | `/api/v2/subscriptions` | — |
| `equidia_get_v2_subscriptions_by_id` | `/api/v2/subscriptions/{id}` | `id` |
| `equidia_get_v2_tags` | `/api/v2/tags` | — |
| `equidia_get_v2_user_optiwebs` | `/api/v2/user-optiwebs` | — |
| `equidia_get_v2_users` | `/api/v2/users` | — |
| `equidia_get_v2_users_by_uuid` | `/api/v2/users/{uuid}` | `uuid` |
| `equidia_get_web_article_mobile` | `/api/web/article-mobile` | `slug` |

## Common aliases

| Alias | Canonical tool |
| --- | --- |
| `equidia_races` | `equidia_get_mobile_races` |
| `equidia_meetings` | `equidia_get_mobile_meetings` |
| `equidia_race` | `equidia_get_mobile_race` |
| `equidia_runners` | `equidia_get_mobile_runners` |
| `equidia_horse` | `equidia_get_mobile_horse` |
| `equidia_horse_stats` | `equidia_get_mobile_horse_stats` |
| `equidia_predictions` | `equidia_get_mobile_daily_predictions` |
| `equidia_live_streams` | `equidia_get_mobile_live_streams` |
| `equidia_tv_schedule` | `equidia_get_tv_epg` |
| `equidia_articles` | `equidia_get_v2_articles` |
| `equidia_tags` | `equidia_get_v2_tags` |

`equidia_api_catalog` searches enabled reads locally. Use `schema TOOL` or `tools` for full parameter descriptions, enums, types and limits.
