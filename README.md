# Guitar Tuner

**Кросс-платформенный гитарный тюнер с двумя независимыми клиентами**

- **Web / PWA:** Vue 3, Web Audio API и `pitch-core`, собранный в WASM.
- **Native desktop:** Rust-приложение на egui/eframe с рендерером wgpu; `audio-input` захватывает микрофон через cpal.

Точный, быстрый и полностью работает без интернета в десктопной версии.

## Что сделано (summary)

- Shared Rust `pitch-core` с разделёнными YIN/MPM/hybrid detector, spectrum, engine, frames и WASM surface.
- `audio-input` crate для native egui: typed cpal formats, mono downmix, bounded recycled chunks и DSP вне realtime callback.
- Discriminated TS `AudioInputPort` и registry для web/synthetic/file adapters; session зависит от capabilities, а exact PCM capture выделен в отдельную capability.
- Full-frame pitch-core/WASM `TunerProcessor` является primary web-worker backend и владеет smoothing/resolution; TS detector остаётся проверенным fallback.
- Worker помечает кадр как `resolved` или `unresolved`, поэтому presentation не определяет полноту кадра по имени backend-а.
- `registry/music-registry.json` является единым источником 14 инструментов и 33 строев для Rust и web.
- Shared B0-E5 fixtures измеряют native/WASM/TS точность в центах; итоговая уверенность дополнительно объединяет периодичность, согласие детекторов, стабильность пяти окон и сигнал/шум, а каждый кадр несёт оценку неопределённости и отпечаток конфигурации.
- Vue 3 web UI с пятью рабочими экранами: Tuner, Library, Practice, Analysis и Algorithm; тяжёлые экраны грузятся отдельными чанками.
- Явный session lifecycle `idle | starting | listening | stopping | error` с сериализацией start/stop/restart и восстановлением после runtime-ошибки.
- Пресеты инструментов и строев, A4, capo, transpose, temperaments, custom tunings, practice history, metronome, themes.
- Waveform/spectrum/spectrogram, cents history, reference tone и ear training.
- Полный `UserProfileV1` import/export для настроек, инструментов, строев, темпераментов, метронома и истории; строгая нормализация и legacy fallback.
- Bilingual RU/EN, keyboard shortcuts, light/colorblind/stage/compact/left-handed/fullscreen modes.
- Production Service Worker с precache реальных hashed chunks и WASM.
- CI/CD отдельно собирает web/PWA и unsigned native egui-пакеты для Windows, macOS и Linux.

## Current Audit / Technical Debt

Текущий порядок работ: [PLAN.md](PLAN.md). Стабильные исторические review-ID и evidence сохранены в [recommendation.md](recommendation.md), а ranked **Top 500 (`M#`) и historical grounded audit (`C#`)** — в [TOP-500-backlog.md](TOP-500-backlog.md).
Все 500 строк также сохранены ниже в README и зеркалах документов. `[DONE YYYY-MM-DD]` означает проверенное закрытие; это реестр идей и рисков, а не утверждение, что 500 независимых функций уже выпущены.

Главные проблемы:
- Instrument/tuning registry и note/cents math имеют единые registry/expression owners; Rust и TypeScript получают проверяемые generated-модули.
- Primary browser WASM и native используют один полный `DetectionFrame`; его bounded `PipelineTelemetry` одинаково показывает YIN/MPM, arbitration, gate/tracker/hold, noise threshold, пять гармоник, три octave hypotheses и итоговое решение без повторного DSP в Vue. TS fallback формирует тот же фиксированный diagnostic shape, а backend остаётся явным. Power flag fallback-а всё ещё намеренно ограничен.
- Web application разделён на framework-independent use cases и явные value contracts в `application/`, UI-facing port contracts в `app/ports/` и Vue-реализации в `adapters/vue/`. Stateful target/hysteresis вычисления заменены явной доменной detection machine; общий `TunerApplicationServices` и вывод типов портов через concrete-factory `ReturnType` удалены. 124-строчный composition adapter только связывает эти части и создаёт input/output/settings adapters. Один `AudioOutputPort` владеет resumed Web Audio context, mixer и отменяемыми scopes; метроном планируется по аудиочасам.
- Web/synthetic/file adapters реализуют один TS `AudioInputPort`; интерактивный WAV-вход декодирует PCM/float без browser resampling, публикует sample-indexed окна и проходит общий session pipeline.
- `useTunerApplication` является composition root и внедряет типизированный `TunerInputSet` в session; lifecycle больше не создаёт конкретные web/file/synthetic adapters. Worker и main-thread fallback используют общий stateful processor и единый protocol contract.
- Spectrum не считается, когда скрыт; native UI публикует immutable `Arc` snapshots. Следующая оптимизация — decimated waveform, меньше spectrum bins и подготовка diagnostics/snapshots вне state mutex.
- Лицензированный корпус из 19 реальных WAV покрывает guitar, bass, ukulele, violin и voice и блокирует CI, включая SNR-сетку 30/20/10 дБ (57/57), reverb-сетку RT60 0.3/0.8/1.5 (57/57), criterion hot-path benchmarks и restart soak. Не хватает более широких visual regression, DSP fuzzing и native renderer telemetry на трёх ОС.
- На вкладке Algorithm работают live route, составная уверенность с реальным разбросом окон, отпечаток конфигурации, диагностика обработки микрофона, предупреждение о конкурирующей струне, decision timeline, freeze/replay inspector, harmonic/octave/noise views, latency budget, virtual bypass и baseline A/B overlay. Debug capture пишет тот же PCM, который анализирует AudioWorklet, и выгружает mono PCM16 WAV + JSON v2 с sample timebase/SHA-256. Rust replay v2 и общий лицензированный контракт сравнивают native Rust/browser WASM кадры и проверяют egui view projection; mic self-test и общая typed error taxonomy остаются открытыми.
- Release hardening (codesign/notarization, Authenticode, checksums и SBOM) остаётся открытым.

Фактическая блок-схема определения высоты, внутренний decision flow и целевой модульный pipeline: [TUNER-PIPELINE.md](TUNER-PIPELINE.md).

Целевая архитектура и фазы рефакторинга: [ARCHITECTURE.md](ARCHITECTURE.md). Практический порядок работ: [PLAN.md](PLAN.md). Подробные рекомендации по рефакторингу: [RECOMMENDATIONS.md](RECOMMENDATIONS.md).

Внешний pitch/DSP-срез 100 репозиториев, научных публикаций и 28 дедуплицированных кандидатов: [RESEARCH-100-PITCH-REPOSITORIES.md](RESEARCH-100-PITCH-REPOSITORIES.md).

Расширенный конкурентный аудит охватил **473 уникальных музыкальных репозитория**: 405 стратифицированных проектов из 18 GitHub topics плюс предыдущие 100, с пересечением 32. Из 139 README и 10 source-tree разборов получены **50 итоговых `G#` предложений** по точности, профессиональному workflow, temperament interoperability, practice, metronome и архитектуре: [RESEARCH-473-MUSIC-REPOSITORIES.md](RESEARCH-473-MUSIC-REPOSITORIES.md). Оба файла являются evidence annex: они усиливают или группируют существующие `M#`/`X#`/`R#`, но не меняют канонический backlog без design/benchmark gate.

## Три итерации и review

1. **Decision evidence:** Rust и TypeScript fallback формируют составную уверенность из четырёх независимых признаков и измеряемый диапазон неопределённости.
2. **Input provenance:** кадр несёт отпечаток pipeline-конфигурации и предупреждение о соседней струне; web-порт отдельно сообщает фактические AGC/noise/echo/sample-rate настройки.
3. **Presentation review:** канонические кадры остаются на аудиочасах, а отдельный visual frame обновляется через `requestAnimationFrame` на частоте дисплея по правилу latest-frame-wins; смысловые переходы проходят сразу, Analysis получил мастер мензуры, а экранный диктор больше не зачитывает меняющийся процент.

Текущий gate на 2026-08-04: 286 Vitest-тестов, 12 Playwright flows, workspace fmt/clippy/tests, production Vue/WASM build, native egui tests, zero-network scan и native/WASM replay parity. Лицензированный corpus `19/19`, SNR-сетка `57/57` и reverb-сетка `57/57` остаются blocking quality evidence.

## Возможности

- Современный алгоритм **YIN** (высокая точность)
- Поддержка нескольких строев (Standard, Drop D, DADGAD, Open G, Open D + свои)
- Реал-тайм визуализация формы волны
- Большой индикатор ноты + шкала центов с гистерезисом
- Интерактивная диагностика алгоритма с timeline, freeze/replay, octave/harmonic evidence, noise floor, latency и A/B what-if
- Мастер настройки мензуры по открытой струне, флажолету и зажатой ноте XII лада
- Клавиатурные сокращения
- Референсный тон
- Полностью оффлайн в десктопной версии
- Кросс-платформенность: Windows, macOS, Linux + браузер

CI и деплой страницы (GitHub Pages для онлайн-версии) настроены так:
- `build-web.yml`, `build-egui.yml`, `security.yml`, `deploy.yml`, `pr-deploy.yml`, `release.yml`
- Страница: https://themoretheless.github.io/tuner/ (включи GitHub Pages с source "GitHub Actions")
- Для десктоп-релизов бинари собираются в release с матрицей (Win/Mac/Linux) и добавляются в GitHub Release
- Страница двуязычная (RU/EN) с переключателем, как в cut-log

Base в web/vite.config.ts = '/tuner/' (repo name is lowercase "tuner").

## Скачать / Собрать

Собери приложение самостоятельно (рекомендуется) или используй CI в GitHub Actions (workflow уже настроен).

## Структура проекта

```
Tuner/
├── audio-input/         # Shared realtime-safe cpal adapter for native shells
├── pitch-core/          # Shared Rust pitch core (YIN + MPM) - used by egui, WASM for web
├── fixtures/            # Shared parity traces plus licensed, checksummed real-WAV corpus
├── registry/            # Canonical instruments/tunings/note-name data
├── scripts/             # Reproducible cross-language note-math code generation
├── web/                 # Vue 3 — онлайн сайт (GitHub Pages)
├── assets/icons/        # Иконки native release-пакетов
├── egui/                # Native desktop (egui/eframe + wgpu + audio-input/cpal)
├── ARCHITECTURE.md      # План рефакторинга + интегрированные бэклоги идей и приоритеты
├── recommendation.md    # Стабильный current extract открытых проблем (R#)
├── TOP-500-backlog.md   # Ranked Top 500 (M#) + historical grounded audit (C#)
├── PLAN.md              # Порядок выполнения и DoD
├── RECOMMENDATIONS.md   # Приоритизированный план исправлений
├── RESEARCH-100-PITCH-REPOSITORIES.md # External evidence + новые X# hypotheses
├── RESEARCH-250-PITCH-SOURCES.md # Source-level разбор + статьи, патенты, форумы
├── RESEARCH-473-MUSIC-REPOSITORIES.md # Competitor scan + финальные 50 G# proposals
├── TUNER-PIPELINE.md    # Фактическая и целевая блок-схемы определения pitch
└── README.md
```

## Как читать проект маленькими кусочками

Проект сейчас лучше разбирать не по клиентским папкам `web/egui`, а по слоям. Это главный SOLID/DRY ориентир:

1. `registry/music-registry.json` и `scripts/generate-note-math.mjs` - два canonical input: музыкальные данные и формулы.
2. `pitch-core/src/generated_note_math.rs`, `web/src/generated/noteMath.ts` - generated primitives; вручную не редактировать.
3. `pitch-core/src/domain.rs` - адаптация `Note`/`Tuning` и тонкий доменный фасад поверх primitives.
4. `pitch-core/src/dsp/{detector,yin,mpm,power}.rs` - независимые detector implementations и fusion policy.
5. `pitch-core/src/resolution.rs`, `engine.rs`, `frames.rs`, `spectrum.rs` - target/hysteresis policy, orchestration и plain `DetectionFrame`.
6. `pitch-core/src/quality/`, `pitch-core/examples/quality/`, `fixtures/corpus/` - независимая оценка готовых traces, corpus runner и provenance; live DSP от них не зависит.
7. `audio-input/src/lib.rs` и `egui/src/audio.rs` - realtime-safe native input, supervised recovery и тонкая связь с `pitch-core`.
8. `egui/src/{app,audio,state,visualization}.rs` - native UI, разделённый по ответственности.
9. `web/src/session/sessionLifecycleContract.ts`, `sessionLifecycle.ts`, `ports/{audioInput,tunerInputSet}.ts`, `adapters/vue/useTunerInputSet.ts`, `composables/useTunerSession.ts` - typed lifecycle, capability contracts, composition web/file/synthetic adapters и session orchestration как отдельные части.
10. `web/src/domain/tuningTracking.ts`, `tuningDetectionMachine.ts`, `composables/useTuningModel.ts`, `useTuningDetection.ts`, `application/controllers/tuning*Commands.ts` - model/commands/detection без stateful computed и смешения причин изменения.
11. `web/src/settings/{settingsState,mergeHydratedSettings}.ts`, `composables/useSettings.ts` - реактивное значение, чистое hydration-слияние и persistence coordinator с внедряемым storage port.
12. `web/src/application/controllers/`, `application/ports/` - framework-independent use cases и минимальные value contracts; Vue импортировать запрещено architecture test-ом.
13. `web/src/ports/audioOutput.ts`, `platform/webAudioOutput.ts`, `domain/metronome.ts`, `application/services/metronomeScheduler.ts` - output capability, Web Audio adapter, чистая арифметика и sample-clock scheduler.
14. `web/src/adapters/vue/{capabilities,controllers,ports}/` и `useTunerApplication.ts` - Vue mapping и composition root; читать последними. `composables/useTuner.ts` оставлен однострочным compatibility facade.

Правило для новых правок: если изменение трогает несколько слоёв сразу, сначала выдели порт, тип или тест. Если код появляется второй раз, это не DRY, а будущий баг.

**Ещё мельче:** свежий пост-рефакторный аудит (2026-07-11) добавил **214 новых пунктов (`R181`-`R394`)** поверх 10 слоёв выше, разложенных по **36 более мелким кусочкам** в тех же 6 группах — таксономия в [ARCHITECTURE.md](ARCHITECTURE.md#finer-grained-piece-map-36-pieces-for-r181) ("Finer-Grained Piece Map"), сами пункты — в [recommendation.md](recommendation.md#post-refactor-findings-r181-r394-by-soliddry-piece). Бери один кусочек (например `Q11 tuning domain`), читай только его файлы и его `R#`.

## Design Direction

Тюнер должен ощущаться как сценический инструмент, а не как панель администрирования. Главный экран обязан сразу отвечать: слушаем ли мы микрофон, какая нота обнаружена, куда крутить струну.

- Live Tuner: большая нота, cents direction, target, mic/level, выбранная струна.
- Practice: ear training, metronome, history, streaks, export stats.
- Library: instruments, tunings, custom profiles, import/export.
- Settings: audio backend/device, A4, temperament, theme, accessibility, diagnostics.

Дизайн-правила: фиксированные размеры для readout/gauge/buttons, минимум визуального шума, no color-only feedback, stage mode с высоким контрастом, compact mode без микротекста, mobile controls в зоне большого пальца.

## Запуск и сборка web

Нужны Node.js 22/npm, Rust 1.96 с target `wasm32-unknown-unknown` и `wasm-pack` 0.15.0. `npm run dev` и `npm run build` сами пересобирают WASM-пакет.

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --version 0.15.0 --locked
cd web
npm ci
npm run dev
```

Локальный URL: <http://localhost:5173>. Production-сборка:

```bash
cd web
npm ci
npm run build
```

Результат находится в `web/dist/`.

## Запуск и сборка native desktop

Нативный клиент собирается из корня Cargo workspace. Node.js и npm для него не нужны.

```bash
cargo run --release -p guitar-tuner-egui
```

Только сборка:

```bash
cargo build --locked --release -p guitar-tuner-egui
```

Бинарник: `target/release/guitar-tuner-egui` (на Windows — `.exe`). На macOS нужны Xcode Command Line Tools, на Windows — MSVC Build Tools. Для Ubuntu/Debian установи native audio/system dependencies:

```bash
sudo apt-get install libasound2-dev libudev-dev pkg-config
```

CI собирает unsigned native-пакеты. Это не installers: Windows получает portable `.zip`, macOS — `.app.zip` для Intel и Apple Silicon, Linux — `.tar.gz` с бинарником, иконкой и README. Иконки находятся в `assets/icons/`.

| Платформа | Текущий release-артефакт |
| --- | --- |
| Browser | GitHub Pages / PWA |
| Windows x86_64 | Portable unsigned `.zip` |
| macOS x86_64 / arm64 | Unsigned `.app.zip` |
| Linux x86_64 | Unsigned `.tar.gz` |

## Как это работает

- Web получает mono samples через `getUserMedia`/Web Audio. `usePitchLoop` передаёт окна в Worker, где работает WASM `TunerProcessor`; TypeScript-детектор остаётся явным degraded fallback.
- Native egui не запускает webview: `audio-input` читает cpal-поток в bounded queue, `pitch-core::TunerEngine` строит `DetectionFrame`, а egui/eframe рисует UI через wgpu.
- Оба клиента делят Rust-ядро и generated music registry, но имеют отдельные UI/audio adapters.

Полный путь от microphone samples до `DetectionFrame`, развилка YIN/MPM/harmonic/octave/tracking и схема диагностики ошибки `E2 -> E3` показаны в [TUNER-PIPELINE.md](TUNER-PIPELINE.md).

## Советы по использованию

- Тихая комната
- Играй по одной струне
- Используй ручной выбор струны для максимальной точности
- Держи гитару близко к микрофону

## Текущий архитектурный статус

Исторические ревью, текущий code-audit и Top 500 сведены в [ARCHITECTURE.md](ARCHITECTURE.md), [recommendation.md](recommendation.md) и едином [TOP-500-backlog.md](TOP-500-backlog.md). README больше не является местом полного аудита.

M0 safety net закрыт для текущего refactor gate: 286 Vitest tests, 12 Playwright flows, лицензированный corpus `19/19`, workspace fmt/clippy/tests, production Vue/WASM build, generated registry/note-math parity, synthetic/file session harness и native/WASM replay parity. Web lifecycle публикует typed start/stop/runtime failures, native recovery проверяется в `audio-input`, а stale web starts отменяются по revision.

Сейчас поддерживаются два shell path: Vue web и egui native. Они делят `pitch-core`, но не один кросс-платформенный session runtime:
- часть domain уже вынесена в `pitch-core/src/domain.rs`;
- `pitch-core` разделён на `domain`, `frames`, `resolution`, `signal`, `smoother`, `engine`, `dsp/{detector,yin,mpm,power}`, `spectrum` и `wasm/{frame,processor}` с совместимой low-level оболочкой;
- web держит тонкий note/temperament facade и explicit pitch fallback; full-frame primary WASM владеет detector/smoothing/resolution, а fallback confidence проверяется тем же manifest;
- egui работает напрямую с `audio-input` и native `TunerEngine`, но пока не в feature parity с web UI;
- native release assets хранятся отдельно от кода в `assets/icons/`.

Нативный egui запуск: `cargo run --release -p guitar-tuner-egui`.

### Offline quality gate

`pitch-core/src/quality/` оценивает временное поведение уже готовых `DetectionFrame`, не импортируя UI, capture adapters или внутренности детектора. Метрики включают подтверждённое время первого корректного захвата, false-lock duration/ratio, reacquisition latency, note switches per second, time-weighted sustain cents MAE и detection coverage.

```bash
cargo run -p pitch-core --example quality -- \
  capture.wav fixtures/quality-scenario.example.json

cargo run --release -p pitch-core --example quality -- \
  --check --corpus fixtures/corpus/manifest.json
```

CLI принимает PCM WAV или raw `f32le`, использует тот же stateful `TunerEngine`, считает timestamp кадра по концу analysis window и печатает versioned JSON для CI/регрессионного сравнения. Corpus manifest задаёт source/output SHA-256, точные license/URL/transform provenance, sample rate, window/hop, tolerance, stable hold, detector range и attack/sustain/release segments. `fixtures/corpus/rebuild.sh` воспроизводимо пересобирает и проверяет все 19 файлов.

Для расследования отдельного capture команда `cargo run --release -p pitch-core --example trace -- capture.wav --json` выгружает versioned sample-indexed envelope с config, candidates, gate/arbitration/hold/tracker и результатом каждого окна.

## Следующие улучшения (рекомендуемые)

- Протянуть аналогичный output-audio capability contract в egui только при работе над реальным cross-platform playback, не связывая platform adapters напрямую.
- Локализовать native egui diagnostics presentation; web typed diagnostics уже готовы.
- Расширить visual-regression suites и DSP fuzzing; criterion/soak/SNR/reverb gates уже выполнены.
- Расширить автоматический exact-PCM replay с трёх лицензированных captures на весь corpus и явно зафиксировать допустимую деградацию TypeScript fallback.
- Закрыть release hardening: signing/notarization, Authenticode, checksums и SBOM.

---

## План глубокого рефакторинга

Полный план (10 критиков + целевая архитектура слоёв + **влитые бэклоги**) — в [ARCHITECTURE.md](ARCHITECTURE.md). Там же приоритезированные пункты из старых бэклогов, SOLID/DRY decomposition и полный Top 500 register.

**Порядок выполнения** — пошаговый план с зависимостями, привязкой к проблемам recommendation.md и критериями готовности — в [PLAN.md](PLAN.md). Это источник истины по очерёдности работ.

**Ключевые цели рефакторинга:**
- Сохранять выполненную controller/port/settings/tuning декомпозицию через автоматические architecture boundaries
- Чёткие слои + трейты (domain / dsp / audio / engine / presentation)
- Визуализаторы получают только plain data (никаких AnalyserNode или cpal утечек)
- Легко тестировать, расширять (MIDI, файлы, новые платформы) и онбордить людей

Мы проектировали **как будто с нуля**, уделяя особое внимание слабой зацепленности между аудио, вычислениями и UI.

Основные native/core/session/UI границы, contextual full-frame WASM/native ownership, controller/port decomposition, общий web output-audio port, confidence/smoothing parity, licensed cross-backend replay, input recovery, music registry, generated note math и file/WAV input выполнены. Дальше — typed diagnostics и quality/release фазы из ARCHITECTURE.md.

## Идеи и предложения (Top 500)

Все существующие бэклоги ([TOP-500-backlog.md](TOP-500-backlog.md) + [IDEAS-round4-500.md](IDEAS-round4-500.md)) **синхронизированы** с [ARCHITECTURE.md](ARCHITECTURE.md).

Там теперь единая живая картина:
- Самые высокоприоритетные P1/P2 пункты из master Top 500 (вытащены наверх).
- Выбранные мощные направления расширения из 500 идей (в первую очередь архитектурно интересные, типа course-aware tuning).
- Полный ranked список из 500 предложений, улучшений, проблем, ошибок и bad-design пунктов ведётся в [TOP-500-backlog.md](TOP-500-backlog.md) и зеркалится ниже в свернутом виде.
- Архитектурная проекция этих 500 пунктов записана в [ARCHITECTURE.md](ARCHITECTURE.md).
- Стабильные текущие problem ids для планирования записаны в [recommendation.md](recommendation.md).

Ключевые направления:
- Завершить слои и убрать coupling (god-объекты, session/audio boundaries, дубли math).
- Весь DSP в pitch-core + реальные оптимизации (decimate, fusion, prealloc, off-main-thread).
- Canvas/DPR + data-driven визуализации (прямо по текущей ветке canvas-dpr-and-dsp-fixes).
- DX и качество (тесты, harness-ы, snapshot-ы, единый источник tuning tables).
- Полировка аудио, accessibility, privacy, PWA и подписанные native-релизы.
- Гитаристские фичи поверх чистой архитектуры (custom tunings, guided flow, history и т.д.).

См. раздел **"Integrated Ideas..."** в ARCHITECTURE.md. 

Работаем так: берём один dependency-ordered slice из [recommendation.md](recommendation.md), сверяем с Top 500, реализуем и проверяем всеми затронутыми targets. `TOP-500-backlog.md` остаётся canonical source, а блок ниже является README-зеркалом. Проверенно закрытые строки помечены `[DONE 2026-07-11]`; остальные не считаются автоматически открытыми P0, пока не прошли повторный grounded audit.

Verified master closures: **M1, M2, M3, M5, M6, M7, M11, M13, M22, M24, M25, M26, M29, M32, M39, M40, M41, M44, M48, M49, M50, M51, M59, M64, M65, M68, M70, M71, M177**.

<details>
<summary>Свернутый Top 500 register</summary>

<!-- TOP500_README:START -->
| M# | Tier | Theme | Item | Note |
| --- | --- | --- | --- | --- |
| M1 | P1 | review | move DSP off cpal realtime callback | [DONE 2026-07-11] |
| M2 | P1 | review | remove blocking Mutex in audio callback | [DONE 2026-07-11] |
| M3 | P2 | review | unify tunings and note math into pitch-core | Registry plus one generated formula owner feed Rust and TypeScript facades. [DONE 2026-07-11] |
| M4 | P2 | review | octave-error guard subharmonic/NSDF |  |
| M5 | P2 | review | real service worker / offline PWA | [DONE 2026-07-11] |
| M6 | P2 | review | eliminate per-callback heap allocations | [DONE 2026-07-11] |
| M7 | P2 | review | check Rust and TS tuning tables match | [DONE 2026-07-11] |
| M8 | P2 | review | code-sign and notarize macOS/Windows |  |
| M9 | P2 | algorithms | Harmonic Product Spectrum octave disambiguator from the existing 2048 FFT | Reuses current FFT to kill octave errors with minimal code. |
| M10 | P2 | review | high-pass filter rumble/mains |  |
| M11 | P2 | review | reconcile Rust/TS frequency-to-MIDI rounding | Both targets use the generated nearest-MIDI contract. [DONE 2026-07-11] |
| M12 | P2 | algorithms | Multi-resolution dual-window analysis: long window for low strings, short for high | Fixes low-E resolution vs high-string latency tradeoff. |
| M13 | P2 | review | stop resizeCanvas every frame | [DONE 2026-07-11] |
| M14 | P2 | review | legacy webview CSP | [REMOVED 2026-08-04] Third shell deleted; hosted-web policy remains separate. |
| M15 | P2 | review | adaptive noise-floor gate |  |
| M16 | P2 | distribution | Verifiable '100% local, no network' privacy badge backed by CI zero-fetch test | Strong trust signal with a cheap CI assertion; differentiates from cloud tuners. |
| M17 | P2 | algorithms | Adaptive per-string tau search bounds derived from the selected target | Faster, fewer-error search when string known. |
| M18 | P2 | review | consolidate five rAF loops into one |  |
| M19 | P2 | review | decouple detection cadence from rAF |  |
| M20 | P2 | review | CI hygiene clippy/rustfmt/deploy-freshness |  |
| M21 | P2 | distribution | Dedicated SEO landing page at /tuner/ targeting 'online guitar tuner' with schema.org FAQ + HowTo | Primary organic-discovery lever for a web tuner. |
| M22 | P2 | dx-quality | WASM/native numeric-equivalence harness over a shared fixture manifest | Native Rust, full-frame browser WASM and TS fallback share B0-E5 cents/confidence fixtures. [DONE 2026-07-12] |
| M23 | P2 | observability-reliability | Graceful-degradation matrix: explicit WASM-down / mic-down fallback states | Defines deterministic UX for every failure mode instead of blank screens. |
| M24 | P2 | docs-dx | Playwright fake-WAV pipeline test asserts detected note | Feed synthetic E2 audio, assert NoteDisplay shows E. [DONE 2026-07-11] |
| M25 | P2 | review | legible sidebar text | [DONE 2026-07-11] |
| M26 | P2 | review | vitest unit tests note math | [DONE 2026-07-11] |
| M27 | P2 | review | one-euro filter |  |
| M28 | P2 | review | WebKitGTK media backend AppImage |  |
| M29 | P2 | review | hardcoded 44100 in egui harmonic overlay | [DONE 2026-07-11] |
| M30 | P2 | algorithms | Confidence-weighted late fusion of YIN, MPM, HPS and Goertzel into one estimate | Single fused estimate from existing detectors cuts octave/jitter errors cheaply. |
| M31 | P2 | a11y-deep | Shape/texture redundancy so in-tune state never relies on color alone | WCAG non-color-reliance; trivial and broadly useful. |
| M32 | P2 | dx-quality | Property-based test for frequencyToNote round-trip across A4 sweep | Deterministic Rust/TS sweeps cover A4, MIDI, cents, temperament and transpose. [DONE 2026-07-11] |
| M33 | P2 | dx-quality | cargo-deny + npm audit supply-chain gate with committed advisory baseline | Blocks vulnerable deps in CI cheaply. |
| M34 | P2 | observability-reliability | "Test My Mic" self-diagnostic wizard with pass/fail panel | Cuts the #1 support cause (no signal) before it becomes a bug report. |
| M35 | P2 | dx-quality | Vitest fake-mic harness driving useTuner via scripted AnalyserNode stub | Deterministic frontend tuner-logic testing. |
| M36 | P2 | observability-reliability | Mic-signal sanity watchdog (silent / clipping / DC-stuck warnings) | Proactively tells users why detection is wrong before they blame the app. |
| M37 | P2 | review | aria-live for note and cents |  |
| M38 | P2 | review | auto-advance string-by-string guided tuning |  |
| M39 | P2 | review | fix CentsHistory deep watcher | [DONE 2026-07-11] |
| M40 | P2 | review | bound MPM NSDF tau range | [DONE 2026-07-11] |
| M41 | P2 | review | chromatic auto-detect mode | [DONE 2026-07-11] |
| M42 | P2 | dx-quality | insta snapshot tests for full DetectionResult on fixture WAVs | [PARTIAL 2026-07-18] 19 real WAV temporal gates exist; per-frame golden snapshots remain. |
| M43 | P2 | i18n-breadth | Browser-language auto-detect via navigator.languages with persisted override | Foundation for all localization; cheap and immediately broadens reach. |
| M44 | P2 | perf-bundle | Preallocate YIN buffers as module singletons across calls | pitch.ts reallocates per size change; pin to max guitar size. [DONE 2026-07-11] |
| M45 | P2 | docs-dx | Playwright E2E for mic-permission-denied flow | Drive fake getUserMedia, assert permission UI path renders. [DONE 2026-07-21] Denial, actionable UI error and successful retry are browser-tested. |
| M46 | P2 | review | localize hardcoded English in-tune hint |  |
| M47 | P2 | algorithms | Goertzel bank locked to 6 selected-string targets and their first 4 harmonics | Cheap targeted detection when string is known. |
| M48 | P2 | perf-bundle | WASM streaming instantiation via instantiateStreaming for pitch-core | wasm-bindgen loader uses `instantiateStreaming`; Playwright verifies the live WASM path. [DONE 2026-07-11] |
| M49 | P3 | review | validate/clamp A4 on load | [DONE 2026-07-11] |
| M50 | P3 | review | gate FFT spectrum when viz hidden | [DONE 2026-07-11] |
| M51 | P3 | review | reuse YIN difference buffers | [DONE 2026-07-11] |
| M52 | P3 | native-os | Native egui mic-permission preflight via macOS AVCaptureDevice request | Avoids silent failure when OS denies mic. |
| M53 | P3 | algorithms | Gaussian-window interpolation on log-magnitude FFT peaks (Jacobsen/Quinn) | Sub-bin frequency accuracy from existing FFT. |
| M54 | P3 | observability-reliability | Stale-PWA / update-available checker against version.json | Stops users getting stuck on cached old builds. |
| M55 | P3 | perf-bundle | Cap maxTau by selected-string frequency to shorten YIN | When string chosen, narrow lag range, fewer CMNDF iterations. |
| M56 | P3 | review | first-run onboarding + mic priming |  |
| M57 | P3 | review | cache Spectrum gradients |  |
| M58 | P3 | review | collapsible settings sidebar on mobile |  |
| M59 | P3 | review | build script copies WASM to unserved dir | [DONE 2026-07-11] |
| M60 | P3 | a11y-deep | Colorblind palette presets (deuteran/protan/tritan) replacing red/green coding | Red/green in-tune coding fails ~8% of male users. |
| M61 | P3 | dx-quality | Golden-trace differential runner: flag any fixture moving >1 cent | [PARTIAL 2026-07-18] Replay envelopes exist; differential baselines and the 1-cent rule remain. |
| M62 | P3 | a11y-deep | Adjustable in-tune tolerance + detection smoothing as accessibility controls | Lets tremor/motor users widen the target. |
| M63 | P3 | i18n-breadth | Locale-correct A4 decimal parsing accepting comma and period keypads | Prevents broken A4 entry for half the world; trivial fix. |
| M64 | P3 | settings-personalization | Configurable in-tune tolerance band in cents | Slider 1-10 cents controls green-zone width directly. [DONE 2026-07-11] |
| M65 | P3 | perf-bundle | Lazy-load Waveform and Spectrum via defineAsyncComponent | App.vue statically imports both; split each into its own chunk. [DONE 2026-07-11] |
| M66 | P3 | review | spectrogram allocation and full redraw |  |
| M67 | P3 | review | pin toolchain and wasm-pack version |  |
| M68 | P3 | review | custom/editable tuning builder | [DONE 2026-07-11] |
| M69 | P3 | review | web AudioWorklet detection |  |
| M70 | P3 | review | strengthen TARGET vs detected note hierarchy | [DONE 2026-07-11] |
| M71 | P3 | review | async device-change restart | [DONE 2026-07-11] |
| M72 | P3 | review | useSettings single source for tuning/A4 |  |
| M73 | P3 | review | in-tune confirmation cue haptic/sound/flash |  |
| M74 | P3 | review | auto-detected string highlight + spring needle |  |
| M75 | P3 | review | maskable PNG icons 192/512 |  |
| M76 | P3 | algorithms | wasm-SIMD f32x4 vectorization of YIN difference and MPM NSDF inner loops | Headroom to run fusion/multi-window without CPU cost. |
| M77 | P3 | a11y-deep | Cognitive-load Simple Mode: target note + big up/down arrow only | Removes clutter for beginners and cognitive accessibility. |
| M78 | P3 | dx-quality | Synthetic harmonic-stack generator with controllable inharmonicity B-coefficient | Realistic test signals for the whole suite. |
| M79 | P3 | algorithms | Real-cepstrum quefrency cross-check gating the YIN result | Independent octave/voicing sanity check. |
| M80 | P3 | native-os | Power/idle-aware stream suspension on display sleep and app hide | Saves battery when not actively tuning. |
| M81 | P3 | observability-reliability | Audio-pipeline health strip (AudioContext state + buffer underrun counter) | Makes silent audio failures visible and debuggable in the field. |
| M82 | P3 | observability-reliability | Sample-rate / device-mismatch reconciliation warning | Explains a subtle, common cause of detection error. |
| M83 | P3 | settings-personalization | Versioned settings schema with migration runner | Stamp schemaVersion; migrate old keys on load, no data loss. |
| M84 | P3 | perf-bundle | Gate visualizer chunk fetch behind showWaveform/showSpectrum toggles | Only download viz code when user enables that visualizer. |
| M85 | P3 | perf-bundle | Decimate input to fixed 22050Hz before YIN loop | Guitar max 400Hz needs no 44.1k; halves tau-search cost. |
| M86 | P3 | review | header wrap/shrink at 320px |  |
| M87 | P3 | review | bass 4/5-string tunings |  |
| M88 | P3 | review | A4 clamp on commit not keystroke |  |
| M89 | P3 | review | reference-tone playback feedback |  |
| M90 | P3 | review | devicechange listener refresh | [DONE 2026-07-21] Debounced refresh preserves a missing saved device and restores it when the catalog recovers. |
| M91 | P3 | review | per-instrument detection frequency range |  |
| M92 | P3 | review | split useTuner god-composable | [DONE 2026-07-19] |
| M93 | P3 | algorithms | Kalman filter on (log-f0, df0/dt) replacing EMA+median smoother | Predictive smoothing tracks vibrato/glide better than EMA. |
| M94 | P3 | dx-quality | Detection-accuracy report artifact: cents-error histogram per SNR bucket | [PARTIAL 2026-07-18] CI uploads per-capture JSON metrics; SNR buckets and histograms remain. |
| M95 | P3 | observability-reliability | Version/build-info panel (git SHA, build date, WASM hash, platform) | Makes bug reports actionable with exact build identity. |
| M96 | P3 | perf-bundle | Brotli + gzip precompress dist with vite-plugin-compression | Static GitHub Pages host can serve .br/.gz for JS/WASM/CSS. |
| M97 | P3 | perf-bundle | Throttle visualizer redraw to 30fps decoupled from detection | Waveform/Spectrum at 30fps saves canvas work, detection stays fast. |
| M98 | P3 | docs-dx | Local seed fixture: bundled reference tone WAVs | Ship per-string sample files for mic-free dev iteration. |
| M99 | P3 | theming-identity | Extract semantic color tokens from hardcoded hex | CSS custom properties layer enabling all theming work in style.css. |
| M100 | P3 | review | redesign StringSelector for narrow column |  |
| M101 | P3 | review | refactor egui App::update god method |  |
| M102 | P3 | review | needle color gradient and directional arrow |  |
| M103 | P3 | a11y-deep | forced-colors mode mapping with SystemColor keywords for canvas needle/spectrum | Windows High Contrast support for canvas elements. |
| M104 | P3 | algorithms | Phase-vocoder instantaneous-frequency refinement of the FFT peak bin | Higher precision than parabolic interpolation. |
| M105 | P3 | distribution | Open Graph + Twitter Card meta with per-tuning dynamic preview image | Better share-link previews for SEO/social. |
| M106 | P3 | observability-reliability | Pseudo-localization CI check flagging hardcoded strings before merge | Stops untranslated strings regressing into releases. |
| M107 | P3 | perf-bundle | Skip getByteFrequencyData when Spectrum component unmounted | Stop analyser frequency reads entirely when spectrum hidden. |
| M108 | P3 | docs-dx | Dev mode synthetic-signal injector toggle | Replace mic with generated f0 for deterministic local UI work. |
| M109 | P3 | review | accessible label for input device select |  |
| M110 | P3 | review | ring buffer for centsHistory |  |
| M111 | P3 | review | tighten 440Hz octave test |  |
| M112 | P3 | review | idle vs no-signal empty states |  |
| M113 | P3 | review | legacy desktop updater feed | [REMOVED 2026-08-04] No updater/network client is shipped. |
| M114 | P3 | algorithms | Inharmonicity-aware f0 fit (B-coefficient stretched-partial model) | Corrects wound-string stretched partials. |
| M115 | P3 | algorithms | pYIN probabilistic candidates + Viterbi HMM pitch track across frames | State-of-art temporal track; heavier to implement. |
| M116 | P3 | a11y-deep | Per-string cents announced as discrete buckets for screen-reader users | Readable SR output instead of rapid numbers. |
| M117 | P3 | i18n-breadth | Translated note-name systems by locale: Do-Re-Mi vs C-D-E | Romance-language users expect solfege; core to feeling native. |
| M118 | P3 | observability-reliability | egui native panic hook writing crash trace to OS app-data dir | Captures native crashes that otherwise vanish. |
| M119 | P3 | i18n-breadth | Translated egui native strings sharing the web app's locale JSON | One translation source covers both clients. |
| M120 | P3 | ui-micro | Press-and-hold +/- A4 stepper auto-repeats with acceleration | Hold accelerates Hz steps; tap nudges single increment. |
| M121 | P3 | perf-bundle | Reuse single Float32Array for RMS and YIN, no copy | Share timeDomainBuffer; avoid second analyser read per frame. |
| M122 | P3 | theming-identity | In-tune color semantics override (green-blind safe sets) | Theme defines in-tune/flat/sharp hues, not hardcoded green/amber. |
| M123 | P3 | review | async load persisted lastTuningId |  |
| M124 | P3 | review | label A4 input |  |
| M125 | P3 | review | label tuning select |  |
| M126 | P3 | review | FFT-accelerate YIN/MPM |  |
| M127 | P3 | dx-quality | DSP scope-recorder: dump per-frame internals to a replayable .ndjson trace | [DONE 2026-07-21] Rust replay v2 compares native/WASM frames and verifies egui projections. |
| M128 | P3 | algorithms | Autocorrelation-of-the-spectrum (spectral autocorrelation) f0 estimator | Extra fusion vote robust to missing fundamental. |
| M129 | P3 | native-os | Window-state persistence across launches | Restores size/position; expected desktop polish. |
| M130 | P3 | a11y-deep | egui native: respect OS reduce-motion/high-contrast via accesskit + theme query | Brings native app to accessibility parity. |
| M131 | P3 | observability-reliability | In-app local error-log viewer with copy-to-clipboard | Users can self-serve logs without devtools. |
| M132 | P3 | i18n-breadth | Locale-aware A4 number formatting with Intl.NumberFormat | Displays decimals correctly per locale; pairs with parsing fix. |
| M133 | P3 | ui-micro | Swipe horizontally on tuner panel to cycle tunings | Touch swipe steps prev/next tuning with edge bounce. |
| M134 | P3 | privacy-security | Clear-all-local-data button wiping localStorage IndexedDB caches | One click clears settings, logs, caches, revokes mic stream. |
| M135 | P3 | offline-storage | Cache-version manifest with stale-cache purge on boot | Compare baked CACHE_VERSION, delete old caches.keys() entries at startup. |
| M136 | P3 | docs-dx | CONTRIBUTING.md with WASM build prerequisites | List wasm-pack, Rust toolchain, npm steps before first run. |
| M137 | P3 | theming-identity | High-contrast pro theme for bright-stage readability | Max luminance separation on note-letter, gauge, cents bar. |
| M138 | P3 | review | anti-aliased decimation |  |
| M139 | P3 | review | ukulele/violin/mandolin/banjo tunings |  |
| M140 | P3 | review | announce listening status to SR |  |
| M141 | P3 | review | redesign PerStringCents |  |
| M142 | P3 | review | clean dead code dev security surface |  |
| M143 | P3 | a11y-deep | Sonification mode: continuous oscillator pitch encodes cents error (beat-against-target) | Enables fully non-visual tuning for blind users. |
| M144 | P3 | design-motion | OKLCH cents-deviation hue ramp as the single tuner color signal | Perceptually-uniform color signal for tuning error. |
| M145 | P3 | native-os | Native macOS/Windows app menu with tuning + A4 items and shortcuts | Standard native menu affordances. |
| M146 | P3 | web-apis | Web Locks to serialize AudioContext/mic across duplicate tabs | Stops two tabs fighting over the mic. |
| M147 | P3 | web-apis | OffscreenCanvas + dedicated Worker for the needle/cents meter | Renders meter off the main thread for smoothness. |
| M148 | P3 | i18n-breadth | Ship named language packs (ES, PT-BR, DE, FR, IT, JA, ZH-Hans, KO, HI, AR) | Concrete locale set turns generic i18n into shippable market reach. |
| M149 | P3 | observability-reliability | One-click diagnostic bundle export (env + flags + recent log, no audio) | Turns vague reports into reproducible ones, privacy-safe. |
| M150 | P3 | i18n-breadth | Regional default A4 preset (442/443 EU orchestral) keyed to locale | Matches local tuning conventions out of the box. |
| M151 | P3 | data-viz | Cents bullseye: concentric tolerance rings with live dot | Dot homes into green center ring as pitch nears target. |
| M152 | P3 | ui-micro | Bottom-sheet tuning picker on mobile with snap points | Swipe-up sheet lists tunings, half/full detents. |
| M153 | P3 | ui-micro | A4 number-stepper with scroll-wheel and arrow-key nudge | Focus field, wheel/arrows adjust Hz within clamp range. |
| M154 | P3 | power-user | URL query params preset tuning/A4/string state | ?tuning=dadgad&a4=442&string=3 deep-links exact state |
| M155 | P3 | power-user | Tab/Shift+Tab roving focus across all controls | Logical focus order, visible focus ring everywhere |
| M156 | P3 | privacy-security | Permissions explainer page detailing microphone-only no-upload usage | Static page explaining mic stays on-device, never transmitted. |
| M157 | P3 | perf-bundle | manualChunks split vendor vue from app code | Vue rarely changes; long-cache vendor chunk separate from app. |
| M158 | P3 | perf-bundle | Cache Spectrum bar gradient and bin-x lookup tables | Precompute bar geometry once per resize, not per frame. |
| M159 | P3 | content-marketing | Public changelog page rendered from version.json | Dated release notes build trust and fresh-content signals. |
| M160 | P3 | docs-dx | ADR for pitch-core as single-source DSP crate | Record why YIN+MPM live in shared Rust, not duplicated per target. |
| M161 | P3 | docs-dx | JSDoc on pitch.ts and notes.ts public functions | Document frequencyToNote, cents math signatures and edge cases. |
| M162 | P3 | review | stage mode large high-contrast readout |  |
| M163 | P3 | review | input-device affordance/heading semantics |  |
| M164 | P3 | distribution | Embeddable iframe widget (vite lib mode) + postMessage onInTune/onNote API | Distribution multiplier across third-party sites. |
| M165 | P3 | design-motion | Spring-physics needle driven by a critically-damped spring integrator | Smooth needle motion without overshoot. |
| M166 | P3 | platform-reach | Android app as a separate future native project | Not part of the current two-client architecture. |
| M167 | P3 | i18n-breadth | Translation-completeness fallback chain with coverage badge | Prevents blank strings and tracks translation progress. |
| M168 | P3 | data-viz | Cents sparkline mini-history under note readout | Tiny 3-second inline trace shows whether pitch is settling. |
| M169 | P3 | ui-micro | Hover popover on string shows target Hz and cents | Desktop tooltip surfaces exact frequency per string. |
| M170 | P3 | ui-micro | Double-tap a string to instantly select as target | Quick gesture pins manual target without dropdown. |
| M171 | P3 | settings-personalization | Units toggle: cents-only vs Hz-and-cents readout | Hide Hz for beginners, show both for techs. |
| M172 | P3 | privacy-security | legacy capability allowlist audit | [REMOVED 2026-08-04] Webview capability layer deleted. |
| M173 | P3 | perf-bundle | Low-end-device mode: halve FFT_SIZE and viz FPS | Detect deviceMemory/hardwareConcurrency, reduce 2048 buffer and redraw rate. |
| M174 | P3 | content-marketing | FAQ schema JSON-LD on landing page | Rich-result eligibility for "is this tuner accurate" queries. |
| M175 | P3 | docs-dx | Two-client architecture diagram in README | pitch-core feeds web WASM and native egui. |
| M176 | P3 | docs-dx | Visual-regression snapshots per CentsGauge needle angle | Lock pixel output of gauge at -50/0/+50 cents. |
| M177 | P3 | review | prefers-reduced-motion handling | [DONE 2026-07-11] |
| M178 | P3 | review | capo/transpose + pitch-pipe per-string reference |  |
| M179 | P3 | a11y-deep | Distinct vibration patterns: pulse-train flat, long-buzz sharp, double-tap in-tune | Eyes-free directional feedback; small code, big inclusion. |
| M180 | P3 | design-motion | Modular type scale + 4px spacing tokens with fluid clamp() root | Design-system foundation for consistent layout. |
| M181 | P3 | a11y-deep | RTL layout support with dir=rtl, logical properties, mirrored cents axis | Supports Arabic/Hebrew UI direction. |
| M182 | P3 | platform-reach | iOS/iPadOS app as a separate future native project reusing pitch-core | Outside current scope. |
| M183 | P3 | observability-reliability | Structured in-app bug-report template prefilled from local diagnostics | Standardizes incoming issues for faster triage. |
| M184 | P3 | i18n-breadth | Translated tuning-preset display names with locale conventions | Completes the localized feel of the catalog. |
| M185 | P3 | ui-micro | Inline-edit string note via tap-to-spin chromatic picker | Tap string label, scroll wheel to reassign note. |
| M186 | P3 | ui-micro | Toast confirming tuning switch with one-tap undo | Transient toast: 'Drop D' with inline undo button. |
| M187 | P3 | perf-bundle | CI bundle-size budget gate on dist JS/WASM bytes | Fail PR if main chunk or WASM exceeds set kB threshold. |
| M188 | P3 | perf-bundle | OffscreenCanvas spectrum compute off main thread, fallback main | Move FFT-bin drawing to worker, free main for detection. |
| M189 | P3 | docs-dx | Histoire/Storybook stories for the 10 Vue components | Isolated CentsGauge, Waveform, Spectrum states without live mic. |
| M190 | P3 | docs-dx | useTuner composable lifecycle sequence diagram | Document AudioContext start, detection loop, teardown ordering. |
| M191 | P3 | privacy-security | Permissions-Policy header denying camera geolocation USB except microphone | Lock down all browser features except needed microphone. |
| M192 | P4 | review | Fretboard SVG keyboard accessible |  |
| M193 | P4 | review | text alternatives/pressed ARIA on canvases |  |
| M194 | P4 | native-os | Always-on-top frameless mini-overlay with click-through when in-tune | Compact stage/DAW companion; high desktop value. |
| M195 | P4 | analytics | Per-session tuning-stability score (0-100 from post-lock cents RMS) | Headline practice metric users can track over time. |
| M196 | P4 | design-motion | CSS container queries on the tuner panel instead of viewport media queries | Correct responsiveness when embedded at any width. |
| M197 | P4 | native-os | Global hotkey nudge-to-next-string in guided tuning | Hands-light stepping during a guided pass. |
| M198 | P4 | native-os | Tray submenu to switch tuning preset and A4 without opening window | Quick config from the system tray. |
| M199 | P4 | a11y-deep | Captions track: on-screen text for every audio cue | Deaf-accessible labeling of sounds. |
| M200 | P4 | a11y-deep | Screen-magnifier Huge Mode with rem-scaled layout and follow-focus reflow | Low-vision large-layout mode. |
| M201 | P4 | instruments-notation | German note-naming (H/B) and Helmholtz/scientific octave toggle | Regional notation convention support. |
| M202 | P4 | observability-reliability | Opt-in privacy-first crash reporter writing a local JSON trace file | Captures real failures without breaking the zero-network guarantee. |
| M203 | P4 | ai-ml-features | On-device 'What tuning is this?' auto-detect from a single open strum | Signature local-ML feature that differentiates from every basic tuner. |
| M204 | P4 | education-content | Lefty mode mirroring the fretboard and string order | Inclusivity win for left-handed players at low cost. |
| M205 | P4 | observability-reliability | Local feature-flag panel persisted to localStorage / config file | Enables safe staged rollout and field debugging. |
| M206 | P4 | i18n-breadth | Localized PWA manifest name/description/shortcuts per language | Native-feeling install metadata per locale. |
| M207 | P4 | brand-microinteractions | Per-string accent identity: each open string owns a fixed brand hue | Consistent visual language across every view. |
| M208 | P3 | ui-micro | Long-press a string opens reference-tone sustain menu | Hold string to ring sustained pitch, release stops. |
| M209 | P3 | ui-micro | Command palette (Cmd/Ctrl-K) for tunings and settings | Fuzzy-search tunings, A4 presets, modes from one input. |
| M210 | P3 | ui-micro | Drag A4 horizontal slider with magnetic 440 detent | Slider snaps softly at 440 within fine drag range. |
| M211 | P3 | power-user | Spacebar toggles listening start/stop | Single most-used action on most accessible key |
| M212 | P3 | settings-personalization | Per-tuning default A4 override | DADGAD remembers 442, standard stays 440 automatically. |
| M213 | P4 | privacy-security | CSP report-only header then enforce wasm-unsafe-eval default-src self | Stage report-only, collect violations, then enforce strict policy. |
| M214 | P4 | perf-bundle | Frame-time budget guard skipping detection when over 16ms | tick() drops a YIN pass on slow frames to hold 60fps. |
| M215 | P4 | offline-storage | IndexedDB tuning store replacing localStorage for packs | Move custom tunings/packs from localStorage to structured IndexedDB store. |
| M216 | P4 | content-marketing | "Standard tuning notes EADGBE" cornerstone SEO page | Targets the single highest-volume beginner guitar query. |
| M217 | P4 | docs-dx | ADR for Vite base '/tuner/' subpath constraint | Document base-path coupling so contributors stop breaking asset URLs. |
| M218 | P4 | docs-dx | Pull request template with target checklist | Boxes for web/egui and shared-core parity. |
| M219 | P4 | docs-dx | ADR for keeping note math in two languages | Explain Rust-TS duplication tradeoff vs single WASM source. |
| M220 | P4 | theming-identity | OLED true-black theme with pure #000 surfaces | Saves AMOLED power; cards become #000, borders dim gray. |
| M221 | P4 | theming-identity | Animated tuning-fork logo with listening/locked states | Tine vibrates while listening, settles when in tune. |
| M222 | P4 | review | footer contrast |  |
| M223 | P4 | review | subtitle contrast |  |
| M224 | P4 | native-os | Single-instance guard that forwards CLI args to the running window | Prevents duplicate windows/mic contention; foundational desktop UX. |
| M225 | P4 | community-social | Curated static community tuning-pack gallery shipped in /public, offline | Content depth with zero backend; high value for low effort. |
| M226 | P4 | ui-micro | Undo/redo stack for tuning and A4 changes | Ctrl-Z reverts last tuning/A4/string edit; redo forward. |
| M227 | P4 | privacy-security | CI no-third-party-requests test blocking external fetch/connect | Playwright fails build if any non-self network request fires. |
| M228 | P4 | review | light theme toggle |  |
| M229 | P4 | a11y-deep | Voice-control-friendly target names like 'tune string E2' | Reliable Voice Control/Dragon targeting. |
| M230 | P4 | workflows | Oboe-A / tuning-fork reference-listen mode that locks to the heard pitch | Match the ensemble's actual sounded A. |
| M231 | P4 | education-content | Fretboard-note quiz overlaying the existing Fretboard SVG | Reuses shipped SVG to add sticky learning value cheaply. |
| M232 | P4 | settings-personalization | Remember-last-string per tuning | Reopen on the string you last tuned in that tuning. |
| M233 | P4 | offline-storage | Full backup export to single .tunerbackup JSON file | Bundle tunings, A4, settings, stats into one downloadable file. |
| M234 | P4 | theming-identity | Illustrated empty-state art for idle/no-signal | SVG sleeping headstock replaces bare idle text states. |
| M235 | P4 | review | strobe tuner mode |  |
| M236 | P4 | native-os | NSStatusItem live cents micro-meter with colored attributed string in menu bar | Glanceable tuning without window focus on macOS. |
| M237 | P4 | native-os | Follow-OS-theme via window theme events feeding web/egui palette | Auto light/dark matching the OS. |
| M238 | P4 | native-os | Native 'in tune, hold it' notification with throttling | Background confirmation without window focus. |
| M239 | P4 | web-apis | WebGPU strobe/phase visualizer with WebGL2 fallback | GPU strobe disc; ambitious visual flourish. |
| M240 | P4 | a11y-deep | Stereo-pan + pitch-glide sonification encoding sharp/flat direction | Directional audio cue beyond single-tone beat. |
| M241 | P4 | a11y-deep | Dyslexia-friendly font option (OpenDyslexic/Atkinson) with spacing | Readability aid; small toggle. |
| M242 | P4 | analytics | Personal in-tune tolerance auto-calibration | Adapts threshold to the user's steadiness. |
| M243 | P4 | distribution | OBS/Twitch browser-source overlay mode (?overlay=1) transparent compact needle | Streamer overlay reusing one query flag. |
| M244 | P4 | education-content | "Tune for this song" preset library with capo + tuning per track | High user-pull feature that drives repeat use and shareable content. |
| M245 | P4 | brand-microinteractions | Tuning-fork wordmark with the dotted 'i' as a vibrating tine | Establishes a memorable visual identity the brand currently lacks. |
| M246 | P4 | i18n-breadth | Per-language pre-rendered landing pages (/tuner/es/, /tuner/de/) with hreflang | Multiplies organic SEO reach across non-English search markets. |
| M247 | P4 | platform-reach | Linux Flatpak on Flathub with PipeWire/portal mic permission | Primary Linux distribution channel with correct mic permissions. |
| M248 | P4 | observability-reliability | Tuning-pack import/export round-trip self-test in CI | Guarantees bandpack compatibility across versions. |
| M249 | P4 | i18n-breadth | ICU MessageFormat plural/gender handling for count strings | Grammatically correct counts across languages. |
| M250 | P4 | monetization | Ko-fi / Buy Me a Coffee one-tap tip jar | Frictionless small-tip path; complements Sponsors. |
| M251 | P4 | bowed-strings | Baroque vs modern A4 quick-toggle 415/430/440 | One-tap historical pitch standards instead of slider hunting. |
| M252 | P4 | data-viz | Cents waterfall: scrolling per-frame deviation history band | Vertical scroll of cents-colored rows shows pluck decay drift. |
| M253 | P4 | ui-micro | Right-click string context menu: set reference, mute, edit | Desktop context menu per string row with actions. |
| M254 | P4 | ui-micro | Long-press A4 value resets to 440 with confirm | Hold value, ripple confirms snap back to standard. |
| M255 | P4 | ui-micro | Keyboard string navigation with up/down and Enter select | Arrow through strings, Enter sets manual target. |
| M256 | P4 | power-user | Number-key direct string selection 1-6 | Press digit to target that string immediately |
| M257 | P4 | power-user | Scriptable JSON config import/export file | Declarative file defines hotkeys, tunings, defaults |
| M258 | P4 | settings-personalization | Quarantine unknown keys on import, never silently drop | Preserve forward-compat keys from newer app versions. |
| M259 | P4 | privacy-security | Subresource Integrity hashes on WASM and JS bundles | Vite plugin emits SRI digests; tamper-proof /tuner/ asset loads. |
| M260 | P4 | perf-bundle | Inline critical CSS, defer rest to cut first paint | Extract above-fold tuner styles, async-load remainder. |
| M261 | P4 | perf-bundle | requestIdleCallback-defer settings/practice code past mic start | Tuner core mounts first; defer TunerControls heavy logic. |
| M262 | P4 | offline-storage | Deferred beforeinstallprompt with contextual re-surface timing | Stash event, show install CTA after second successful tune. |
| M263 | P4 | content-marketing | Per-tuning explainer article set: Drop D, DADGAD, Open G | One deep page per tuning with notes, songs, history. |
| M264 | P4 | content-marketing | Tuning frequency reference table page (Hz per string) | Snippet-bait table for 82.41Hz E2 etc. queries. |
| M265 | P4 | content-marketing | Reddit r/Guitar launch + AMA-style demo thread | Privacy/offline angle resonates with that community. |
| M266 | P4 | docs-dx | Issue template for new tuning-preset submissions | Structured form: strings, frequencies, source citation. |
| M267 | P4 | docs-dx | l10n contributor guide for adding string keys | Document l10n.ts structure and RU/EN key parity rules. |
| M268 | P4 | review | surface color tokens remove unused palette |  |
| M269 | P4 | review | viz start/stop transitions |  |
| M270 | P4 | distribution | Chrome/Firefox MV3 toolbar extension opening WASM tuner in 360px popup | New surface reusing existing pitch-core build. |
| M271 | P4 | analytics | Per-string accuracy heatmap across the 6 string targets | Cheap visualization surfacing weak strings. |
| M272 | P4 | native-os | Global push-to-tune hotkey that summons overlay only while held | On-demand overlay for live performance. |
| M273 | P4 | web-apis | navigator.storage.persist() + estimate() to mark packs non-evictable, warn on low quota | Protects saved tunings from eviction. |
| M274 | P4 | community-social | User-submitted tuning presets via GitHub PR with JSON schema + CI validation | Crowdsources the catalog safely; classic OSS growth loop. |
| M275 | P4 | brand-microinteractions | Swappable needle skins: Strobe Disc, Analog VU, Laser Line, Vintage Plate | Personalization driver and a natural Pro-tier upsell candidate. |
| M276 | P4 | platform-reach | Windows Store MSIX package with packaged-app mic capability | Clean Windows install and store discovery with proper capabilities. |
| M277 | P4 | education-content | Genre/artist-themed tuning collections as grouped catalog sections | Improves discoverability of the existing tuning catalog. |
| M278 | P4 | i18n-breadth | Arabic RTL needle/cents with mirrored layout but LTR pitch axis | Correct bidi handling so the meter stays physically meaningful. |
| M279 | P4 | monetization | GitHub Sponsors tier ladder with in-app 'Sponsor' footer link | Low-effort recurring support channel for an OSS project. |
| M280 | P4 | brand-microinteractions | Idle 'breathing' needle animation when no signal is present | Signals the app is alive and listening; cheap polish. |
| M281 | P4 | live-deep | Single-string isolation: lock detection to one target | Ignore other strings when tech tunes one string fast. |
| M282 | P4 | ui-micro | Segmented control for detection mode guitar/chromatic/strobe | Sliding pill toggle replaces dropdown for modes. |
| M283 | P4 | settings-personalization | Export full config as downloadable tuner.json | Serialize all keys including custom tunings to one file. |
| M284 | P4 | privacy-security | Cross-Origin-Isolation COOP COEP headers for hardened context | Enable crossOriginIsolated, gate future SharedArrayBuffer DSP safely. |
| M285 | P4 | content-marketing | Comparison page: Tuner vs GuitarTuna/Fender Tune | Privacy/offline/free angle captures branded comparison search. |
| M286 | P4 | docs-dx | Design-token reference page from Tailwind config | Auto-render color/spacing tokens used across components. |
| M287 | P4 | theming-identity | Gauge face skins: arc, linear bar, half-circle dial | Pluggable CentsGauge rendering bound to one theme choice. |
| M288 | P4 | analytics | Blind-tuning self-test (hide cents, score the guess) | Ear-training metric distinct from games already shipped. |
| M289 | P4 | native-os | tuner:// protocol handler opening a specific tuning + A4 preset | Deep-launch into a configured state from links. |
| M290 | P4 | web-apis | Web MIDI input tuning mode: cents deviation of MIDI note-on vs A4 | Tune from a connected keyboard/controller. |
| M291 | P4 | brand-microinteractions | Signature in-tune chime voiced from the current tuning's open strings | Audio branding moment that reinforces success feedback. |
| M292 | P4 | i18n-breadth | Locale-correct font stack for CJK/Arabic/Devanagari with subset fonts | Without this the language packs render as tofu; gating dependency. |
| M293 | P4 | brand-microinteractions | User color-theme creator: 2-color seed generates the full dark palette | Strong personalization but needs a robust palette-generation engine. |
| M294 | P4 | platform-reach | macOS App Store (MAS) sandboxed distribution channel | Sandboxed channel that pairs with paid IAP on macOS. |
| M295 | P4 | community-social | In-app 'Suggest a tuning' button prefilling a GitHub issue/PR body | Lowers the contribution barrier for the PR-based catalog. |
| M296 | P4 | brand-microinteractions | In-tune celebration micro-burst: subtle particle bloom on lock | Delightful reward moment reinforcing success. |
| M297 | P4 | bowed-strings | Cello C-string low-end detection range extension | Reliable f0 down to bass C1 32.7 Hz. |
| M298 | P4 | ui-micro | Inline toast queue stacking with auto-dismiss timers | Multiple notifications stack, oldest expires first. |
| M299 | P4 | power-user | Quick-switch tuning palette (Ctrl+K command bar) | Fuzzy-search overlay to jump to any tuning instantly |
| M300 | P4 | power-user | Hold-to-sound reference tone while key down | Momentary tone playback released on keyup |
| M301 | P4 | settings-personalization | Import config JSON with validation and diff preview | Validate against schema, show what changes before applying. |
| M302 | P4 | settings-personalization | Advanced vs Simple settings disclosure split | Hide gate/tolerance/range behind an Advanced toggle. |
| M303 | P4 | content-marketing | Glossary pages: cents, A4, harmonics, intonation | Long-tail definitional pages internally linking to tuner. |
| M304 | P4 | content-marketing | "Best A4 reference: 440 vs 432 vs 442" debate article | Controversial topic drives shares and backlinks. |
| M305 | P4 | notifications-engagement | New-feature announcement modal keyed to version.json | Show once per build SHA; dismiss persists in localStorage. |
| M306 | P4 | review | TuningSelector redundant label |  |
| M307 | P4 | review | metronome tap-tempo accent |  |
| M308 | P4 | distribution | GitHub Action auto-generating animated demo GIF via headless Chromium + synthetic audio | Keeps README/store demo fresh automatically. |
| M309 | P4 | workflows | Pre-take tuning gate with pass/fail threshold for the engineer | Blocks recording until in-tune; high studio value, low effort. |
| M310 | P4 | analytics | Time-to-in-tune metric per string and per session | Quantifies tuning speed improvement over time. |
| M311 | P4 | web-apis | File System Access .tunerpack save/open with persistent FileSystemFileHandle | Edit-in-place tuning packs on the web. |
| M312 | P4 | analytics | First-attempt overshoot detector (sharp/flat bias profile) | Reveals systematic tuning bias. |
| M313 | P4 | pro-audio-ecosystem | VST3 + AU bundle via nih-plug reusing pitch-core unchanged | Largest reach multiplier for the existing engine into producer workflows. |
| M314 | P4 | education-content | Scale-practice mode detecting each played degree against a chosen scale | Extends the detector into practice tooling without new DSP. |
| M315 | P4 | education-content | Chord-library cross-reference keyed to the current tuning | Very useful for alt tunings; large content and correctness burden. |
| M316 | P4 | platform-reach | ChromeOS-optimized installable PWA with tablet/clamshell mic handling | Captures the large education Chromebook base cheaply. |
| M317 | P4 | education-content | Open-string note-name recognition trainer (E-A-D-G-B-e flashcards) | Cheap beginner drill that reinforces fundamentals. |
| M318 | P4 | community-social | Localized community-translations credit page + i18n CONTRIBUTING guide | Motivates and structures translator contributions. |
| M319 | P4 | bowed-strings | Bowed-string preset bank GDAE/CGDA/CGDA-bass tunings | Violin, viola, cello, 4/5-string bass standard fifths sets. |
| M320 | P4 | live-deep | Loud-stage noise-aware confidence floor | Adapt gating thresholds for ambient stage roar between songs. |
| M321 | P4 | data-viz | Beat-frequency envelope meter vs reference tone | Pulsing amplitude bar; beat rate slows to zero at unison. |
| M322 | P4 | ui-micro | Drag-reorder strings to reverse for left-handed display | Vertical drag handle reorders string list, persists. |
| M323 | P4 | power-user | Keyboard cheat-sheet overlay bound to '?' | Modal listing all active shortcuts contextually |
| M324 | P4 | settings-personalization | Named setting presets (Studio, Live, Practice) | Save full settings snapshot under a name, switch instantly. |
| M325 | P4 | settings-personalization | Reset-to-defaults scoped per settings section | Reset only visualizers or only detection, not everything. |
| M326 | P4 | settings-personalization | Cloud-free settings sync via shareable text blob | Base64 paste-string moves config between browser and native. |
| M327 | P4 | privacy-security | Dependency pinning by integrity hash plus lockfile-lint gate | lockfile-lint enforces https resolved URLs and integrity present. |
| M328 | P4 | offline-storage | Versioned IndexedDB schema with onupgradeneeded migration ladder | Sequential migration functions per schema version, idempotent and tested. |
| M329 | P4 | content-marketing | "Why does my guitar go out of tune" troubleshooting article | High-intent maintenance query with strong app CTA. |
| M330 | P4 | content-marketing | YouTube short: 30-second offline-tuner demo | Visual proof of accuracy for social distribution. |
| M331 | P4 | content-marketing | breadcrumb + Article schema on all explainer pages | Structured data lifts SERP presentation site-wide. |
| M332 | P4 | docs-dx | commitlint config rejecting type: prefixes | Enforce the repo's no-conventional-prefix subject convention in CI. |
| M333 | P4 | theming-identity | Vintage analog-meter skin with cream face and amber lamp | Skeuomorphic needle, ticks, glow for the gauge component. |
| M334 | P4 | review | normalize corner radii |  |
| M335 | P4 | workflows | Setlist-bound multi-guitar profiles with one-tap silent-stage switch | Targets gigging players changing tunings between songs. |
| M336 | P4 | workflows | Studio tuning log: timestamped take/tuning entries per session | Engineer-facing record of tuning at each take. |
| M337 | P4 | analytics | Drift-after-tuning timeline per string (settle curve overlay) | Shows new-string settle behavior visually. |
| M338 | P4 | web-apis | Media Session now-playing surface for active tuning with prev/next-string actions | Lock-screen/headset string stepping. |
| M339 | P4 | web-apis | SpeechRecognition voice commands: 'next string','low E','play A','stop' | Fully hands-free web operation. |
| M340 | P4 | education-content | "Your first 4 chords" guided lesson path using the detector | Onboards absolute beginners; content-heavy to do well. |
| M341 | P4 | hardware-peripherals | Guitar-with-USB direct-input device profile with auto channel selection | Smooths setup for USB-equipped guitars and interfaces. |
| M342 | P4 | community-social | Contributor wall generated at build time from git history (Credits page) | Recognition fuels OSS contribution; fully automated. |
| M343 | P4 | ai-ml-features | Ship on-device models as versioned WASM/ONNX with cache + integrity check | Infra prerequisite for any shipped local-ML feature. |
| M344 | P4 | bowed-strings | Per-string fifths-check mode for violin/viola/cello/bass | Tune adjacent strings as pure 3:2 fifths, beat-rate readout. |
| M345 | P4 | live-deep | Drop-tune delta: cents-to-detune for low string | Show how far to slacken E to D for next song. |
| M346 | P4 | data-viz | Lissajous phase figure: mic signal vs reference sine | Rotating ellipse freezes still when string matches reference frequency. |
| M347 | P4 | ui-micro | Two-finger swipe-down dismisses settings sidebar mobile | Gesture closes panel matching native sheet feel. |
| M348 | P4 | power-user | egui native global keymap mirroring web bindings | Shared keymap JSON consumed by egui input handler |
| M349 | P4 | settings-personalization | Settings dirty-state and discard-changes guard | Warn before nav if unsaved manual edits exist. |
| M350 | P4 | settings-personalization | Default-startup-view setting (tuner/ear-trainer) | Choose which mode opens on launch. |
| M351 | P4 | offline-storage | storage.persisted() request gated on engagement signal | Request persistent storage after user saves first custom tuning. |
| M352 | P4 | content-marketing | Tutorial series: tune by ear without a tuner | 5th-fret method article funnels to app as backup. |
| M353 | P4 | content-marketing | "Drop D vs Drop C vs Drop B" comparison cluster | Metal-genre tuning cluster captures niche long-tail. |
| M354 | P4 | theming-identity | Custom accent picker from a color wheel | Replace fixed #22c55e green across buttons, gauge, strings. |
| M355 | P4 | theming-identity | Curated built-in theme gallery picker in settings | Thumbnail grid of bundled themes with live preview swatch. |
| M356 | P4 | notifications-engagement | Notification permission soft-ask after first session | Explain value before triggering OS permission prompt. |
| M357 | P4 | distribution | App Store / Play Store listing asset kit generator from a single template | Automates icon/screenshots/ASO copy for store launch. |
| M358 | P4 | native-os | File association for .gtuning custom-tuning files with open-with import | Double-click import of shared tunings. |
| M359 | P4 | workflows | Session export of tuning log to CSV for studio/teacher records | Portable records from the log. |
| M360 | P4 | community-social | Exportable .bandpack: signed bundle of tunings + A4 + per-string references | Solves real band-coordination pain and seeds a sharing format. |
| M361 | P4 | platform-reach | Android Quick Settings tile + iOS Control Center/Lock Screen launcher | One-tap access drives habitual use on mobile. |
| M362 | P4 | hardware-peripherals | Clip-on contact piezo input profile with vibration-pickup auto-detect | Better noisy-stage tuning for the common clip-on use case. |
| M363 | P4 | education-content | Interval ear-training between two played strings | Useful musicianship feature reusing detection; not a game per se. |
| M364 | P4 | education-content | Note-on-staff reader linking each open string to standard notation | Connects tuning to notation literacy for learners. |
| M365 | P4 | ai-ml-features | Local capo/partial-capo detector from open-string set vs chosen tuning | Auto-transposes targets; reuses existing detection output. |
| M366 | P4 | bowed-strings | Double-bass fourths tuning EADG preset | Orchestral bass tunes in fourths, not fifths; distinct table. |
| M367 | P4 | bowed-strings | Bow-noise tolerant gating for sustained bowed tone | Stable readout despite scratchy attack and bow changes. |
| M368 | P4 | kids-gamify | Kids mode toggle: oversized 56px+ string buttons | Big touch targets, fewer controls, hides advanced panels |
| M369 | P4 | data-viz | Polar pitch wheel with 12 semitone spokes | Detected note as rotating arm; cents push off-spoke radially. |
| M370 | P4 | data-viz | Pitch trajectory comet: fading trail of recent f0 | Comet tail shows attack glide direction toward target line. |
| M371 | P4 | power-user | Focus mode hiding all chrome (key 'f') | Hide header/footer/sidebar, show only needle |
| M372 | P4 | power-user | Hotkey to cycle reference-tone through all strings | Bracket keys step pitch-pipe up/down strings |
| M373 | P4 | settings-personalization | Per-string custom in-tune tolerance overrides | Tighter band on high E, looser on low E. |
| M374 | P4 | privacy-security | Static asset hash manifest verified against version.json at load | Runtime checks served bundle hashes match signed manifest. |
| M375 | P4 | offline-storage | Backup schema-version field with forward-compat import guard | Reject or migrate older/newer .tunerbackup versions with clear message. |
| M376 | P4 | content-marketing | Press/media kit page: logo, screenshots, copy blurbs | Lowers friction for bloggers and reviewers to feature. |
| M377 | P4 | content-marketing | Song-to-tuning index page (capo + tuning per song) | Curated static map of popular songs to their tunings. |
| M378 | P4 | theming-identity | Live theme preview before applying in picker | Hover a theme tile to temporarily recolor the tuner. |
| M379 | P4 | notifications-engagement | Native egui scheduled practice reminder | Requires an explicit OS integration design; no tray plugin is shipped. |
| M380 | P4 | business-ops-deep | Canary channel toggle pulling versioned WASM from /tuner/canary/ | Opt-in users get prerelease builds before stable promotion. |
| M381 | P4 | instruments-notation | Per-tuning notation-system binding so world presets auto-select naming scheme | Right notation appears automatically with preset. |
| M382 | P4 | workflows | Capo-aware shared key for the band: announce capo + sounding key | Aligns capoed players on a key. |
| M383 | P4 | analytics | Practice streak + calendar heatmap (GitHub-style) | Habit motivation via streak grid. |
| M384 | P4 | pro-audio-ecosystem | CLAP-format tuner plugin sharing pitch-core as DSP backend | Modern open plugin format; pairs naturally with the VST3/AU build. |
| M385 | P4 | hardware-peripherals | USB-HID footswitch mapping for hands-free next-string stepping | High value for live performers; modest native-app effort. |
| M386 | P4 | ai-ml-features | Auto-tab a short monophonic riff into ASCII tablature, fully on-device | Standout local feature, but scope and accuracy risk are large. |
| M387 | P4 | education-content | Printable tuning + chord-chart practice sheet PDF generator | Tangible takeaway for teachers and students. |
| M388 | P4 | ai-ml-features | On-device practice-session auto-summary from the local drift timeline | Gives end-of-session value from already-logged data. |
| M389 | P4 | monetization | Affiliate gear links: contextual string/capo/pickup recommendations | Passive revenue tied to relevant moments; keep tasteful. |
| M390 | P4 | bowed-strings | Fine-tuner vs peg guidance by cents magnitude | Coarse error says peg, small error says fine-tuner. |
| M391 | P4 | plucked-world | Course-aware Tuning model for paired-string instruments | Add course grouping so bouzouki/laud/mandola octave pairs map correctly. |
| M392 | P4 | wind-brass | Long-tone intonation-hold scoring with drift graph | Score steadiness over a sustained note, plot cents over seconds. |
| M393 | P4 | live-deep | Stage-blackout one-hand mode: giant edge tap zones | Full-screen left/right halves advance string, no precise targets. |
| M394 | P4 | data-viz | Six-string radial gauge cluster, hexagon arrangement | All EADGBE mini needles at once for whole-guitar glance. |
| M395 | P4 | data-viz | Confidence ribbon overlaid on cents trace | Trace thickness or opacity encodes detection confidence per frame. |
| M396 | P4 | ui-micro | Radial long-press menu around string: tone, edit, octave | Hold spawns arc of actions under finger. |
| M397 | P4 | settings-personalization | Per-device input profiles keyed by deviceId label | Auto-load A4/tolerance/gate when a known mic reconnects. |
| M398 | P4 | privacy-security | CycloneDX SBOM generation for npm and Cargo dependencies | Emit signed SBOM artifact per release for npm and crates. |
| M399 | P4 | perf-bundle | Subset Tailwind font stack, drop unused system-ui fallbacks | No custom font loaded; trim CSS and preconnect nothing. |
| M400 | P4 | offline-storage | Backup restore with dry-run diff preview | Show added/changed/removed entries before committing restore. |
| M401 | P4 | integrations-music | Read tuning from imported Guitar Pro .gp/.gpx file | Parse .gp track header, auto-select matching 6-string tuning. |
| M402 | P4 | content-marketing | "How to tune a 12-string guitar" long-form guide | Octave-pair tuning is a high-intent unanswered query. |
| M403 | P4 | content-marketing | Embeddable "Tuned with" badge for guitar blogs | Backlink-generating HTML snippet pointing to /tuner/. |
| M404 | P4 | content-marketing | Social share-card SVG templates per tuning result | Brandable images for Reddit/forum tuning posts. |
| M405 | P4 | docs-dx | iframe widget embed API reference page | Document postMessage events, allowed attributes, sizing contract. |
| M406 | P4 | docs-dx | Copy-paste iframe embed snippet generator page | Interactive form emitting ready iframe HTML for sites. |
| M407 | P4 | theming-identity | Sepia warm low-blue-light reading variant | Amber-tinted surfaces for late-night practice eye comfort. |
| M408 | P4 | business-ops-deep | Donation thermometer SVG fed by static goals.json | Server-maintained JSON renders raised-vs-goal bar, no tracking. |
| M409 | P4 | workflows | Concert-A broadcast: one device sets reference pitch for the whole ensemble | Solves real orchestra/band reference-pitch coordination. |
| M410 | P4 | workflows | Teacher push-a-target mode: instructor sets note, student screen mirrors | Remote-lesson tuning sync; strong teaching hook. |
| M411 | P4 | web-apis | Gamepad API foot-controller stepping to advance strings/toggle reference hands-free | Hands-free control for performers. |
| M412 | P4 | workflows | Luthier string-change log with brand/gauge and settle-in tracking | Records string history for setup work. |
| M413 | P4 | pro-audio-ecosystem | AUv3 app-extension inside a thin iOS host wrapper | Lets iOS DAW users tune inline; depends on the iOS build landing first. |
| M414 | P4 | community-social | Local family/band profiles (avatar + name) in IndexedDB, header-switchable | Personalizes multi-user devices without any account system. |
| M415 | P4 | ai-ml-features | Smart string-change reminder from accumulated post-tuning drift trend | Turns drift history into a useful maintenance nudge. |
| M416 | P4 | ai-ml-features | On-device model-card + provenance page (no phone-home guarantee) | Builds trust for local-ML features; reinforces privacy brand. |
| M417 | P4 | wind-brass | Difference-tone / beat-rate visualizer against reference drone | Show beating against sustained reference for unison wind tuning. |
| M418 | P4 | vocal-training | Sustained-note steadiness meter (cents standard deviation) | Live wobble gauge from rolling f0 variance during one held note. |
| M419 | P4 | kids-gamify | Reward chime built from the open-string chord | Reuse sine engine to play a happy arpeggio on success |
| M420 | P4 | data-viz | Harmonic stack ladder: partial deviations vs ideal integers | Visualizes string inharmonicity as drift up the overtone ladder. |
| M421 | P4 | power-user | Command palette recent/favorites ordering | Surface last-used tunings first in Ctrl+K list |
| M422 | P4 | settings-personalization | Settings JSON schema doc generated from TS types | Single source describes every key for import validators. |
| M423 | P4 | privacy-security | CI fail on disallowed WASM imports outside known namespace | wasm-objdump asserts only env/webaudio imports, no surprise host calls. |
| M424 | P4 | perf-bundle | Merge favicon.svg + icons.svg into one symbol sprite | Two SVGs (14KB) become one cached <use> sprite request. |
| M425 | P4 | theming-identity | Per-instrument auto-theme keyed to selected tuning | Acoustic warm-wood vs metal cold-steel palette per preset. |
| M426 | P4 | theming-identity | Themeable needle/pointer SVG asset packs | CentsGauge pointer loads from skin set: blade, dial, dot. |
| M427 | P4 | theming-identity | Theme import/export as single shareable JSON file | Tokens serialized to .gtheme for swap without a server. |
| M428 | P4 | theming-identity | Per-string accent ramp themeable as a gradient set | Six string hues derive from one editable base ramp. |
| M429 | P4 | notifications-engagement | egui native reminder via notify-rust desktop toast | Standalone egui app schedules its own OS notification. |
| M430 | P4 | business-ops-deep | In-app roadmap voting via GitHub Discussions reactions embed | Read-only fetch of reaction counts, vote opens GitHub. |
| M431 | P4 | business-ops-deep | Help-desk widget linking to canned offline troubleshooting answers | Bundled FAQ, deep-links to email with diagnostics prefilled. |
| M432 | P4 | data-viz | Chromagram: 12-bin pitch-class energy bar ring | Folds spectrum into pitch classes; confirms fundamental over harmonics. |
| M433 | P4 | data-viz | Cents histogram building live during a hold | Bars accumulate; symmetric narrow peak means stable in-tune hold. |
| M434 | P4 | privacy-security | egui native config file written with 0600 restrictive permissions | Chmod app-data tuning config so other users cannot read. |
| M435 | P4 | integrations-music | Drop-link bar: paste any tab URL, extract tuning | Unified parser dispatching to Songsterr/UG/GP by host. |
| M436 | P4 | instruments-notation | Sargam note-naming (Sa Re Ga Ma Pa Dha Ni) with movable Sa | Core Indian-classical notation; unlocks that audience. |
| M437 | P4 | hardware-peripherals | Serial/JSON local control protocol as the integration contract for peripherals | Foundation enabling every footswitch/LED/Stream Deck peripheral cleanly. |
| M438 | P4 | hardware-peripherals | Elgato Stream Deck plugin: tuning-select, A4-nudge, live cents on keys | Reaches the streamer/creator niche; depends on the control protocol. |
| M439 | P4 | hardware-peripherals | USB MIDI-controller knob/pad mapping (input only) for tuning and A4 | Reuses MIDI-input infra for hands-free hardware control. |
| M440 | P4 | bowed-strings | Natural-harmonic target mode (5th/4th nodes) | Tune by lightly-touched harmonics, expected pitch shown. |
| M441 | P4 | studio-deep | Per-string offset profile saved as session tuning preset | Snapshot exact measured cents per string, recall next day. |
| M442 | P4 | live-deep | Silent between-song mode: vibrate-only, screen dimmed | No audio reference, haptic-only confirmation for quiet tuning. |
| M443 | P4 | kids-gamify | Tuney the tuning-fork mascot reacts to cents error | SVG sprite wobbles flat/sharp, smiles when string lands in tune |
| M444 | P4 | data-viz | Session timeline scrubber over recorded tuning attempt | Drag playhead across a stored cents-vs-time curve per string. |
| M445 | P4 | settings-personalization | Settings search/filter box | Type to jump to any control across all sections. |
| M446 | P4 | settings-personalization | Settings change-history with undo stack | Step back through recent setting edits this session. |
| M447 | P4 | settings-personalization | Preset auto-apply rule by connected device | Bind a named preset to fire when a mic appears. |
| M448 | P4 | privacy-security | security.txt at well-known with contact and PGP | Publish /tuner/.well-known/security.txt for vulnerability disclosure. |
| M449 | P4 | offline-storage | Storage-usage meter UI in settings sidebar | navigator.storage.estimate() usage/quota bar with per-category breakdown. |
| M450 | P4 | offline-storage | Quota-pressure handler degrading non-essential caches first | On QuotaExceededError evict spectrogram caches before tuning data. |
| M451 | P4 | integrations-productivity | Apple Calendar practice reminder via generated .ics download | Export VEVENT with VALARM for next practice session. |
| M452 | P4 | integrations-music | Songsterr paste-link tuning extractor | Paste Songsterr URL, fetch track tuning JSON, apply preset. |
| M453 | P4 | content-marketing | Email newsletter: monthly tuning tip + changelog | Re-engagement channel; static signup, no backend needed. |
| M454 | P4 | theming-identity | Swappable icon-set variants outline/filled/duotone | Mic, settings, play icons share one selectable style family. |
| M455 | P4 | notifications-engagement | Quiet-hours window suppressing all reminders | User-defined start/end; clamp scheduled times outside band. |
| M456 | P4 | notifications-engagement | Do-not-disturb master toggle pausing all nudges | One switch silences reminders for a chosen duration. |
| M457 | P4 | notifications-engagement | Native autostart with minimized tray for reminders | Deferred OS-specific integration. |
| M458 | P4 | business-ops-deep | Privacy-preserving local aggregate metrics with k-anonymity batching | Opt-in counters flushed only above threshold, no IDs. |
| M459 | P4 | business-ops-deep | Self-hosted Plausible-style aggregate dashboard, IP-truncated, opt-in | First-party analytics with no cookies or persistent IDs. |
| M460 | P4 | bowed-strings | Equal-tempered vs pure-fifths deviation display | Show both ET target and beatless-fifth target cents. |
| M461 | P4 | kids-gamify | Daily challenge: tune all six before timer ends | One seeded challenge per local date, completion badge |
| M462 | P4 | kids-gamify | Confetti bloom and mascot cheer on six-string completion | CSS particle burst when all strings tuned in session |
| M463 | P4 | kids-gamify | Star rating per string: 1-3 stars by tuning precision | Tighter cents window earns more stars, drives replay |
| M464 | P4 | privacy-security | Privacy regression snapshot of localStorage keys in CI | Golden test fails if new persisted key appears unreviewed. |
| M465 | P4 | offline-storage | Eviction warning when persisted-storage permission denied | Banner noting data may be cleared under disk pressure. |
| M466 | P4 | integrations-music | Ultimate-Guitar tab URL capo/tuning sniffer | Read UG page tuning+capo line, suggest matching tuner setup. |
| M467 | P4 | instruments-notation | Oud course tuning presets (Arabic, Turkish, Iraqi) with 5-6 double courses | Opens a large underserved Middle-Eastern player base. |
| M468 | P4 | instruments-notation | Maqam quarter-tone target set (24-TET / koma) with named jins | Microtonal targets for Arabic/Turkish music. |
| M469 | P4 | instruments-notation | Harp / autoharp full-range chromatic per-string tuning sequencer | Sequenced many-string tuning workflow. |
| M470 | P4 | instruments-notation | Hammered/mountain dulcimer and bouzouki/charango course tunings | Folk course-instrument presets, low effort. |
| M471 | P4 | monetization | Paid Pro feature bundle definition and pricing page | The 'what is Pro' anchor every monetization idea depends on; define first. |
| M472 | P4 | monetization | Desktop app on Microsoft Store / Mac App Store with paid Pro IAP | Native store discovery plus a sanctioned IAP monetization channel. |
| M473 | P4 | pro-audio-ecosystem | ARA2 plugin placing per-note tuning markers along the DAW timeline | Deep DAW integration for editors; significant host-specific work. |
| M474 | P4 | hardware-peripherals | Generic USB gamepad/foot-pedal stepping via gilrs (native) | Cheap hands-free stepping reusing a standard input library. |
| M475 | P4 | pro-audio-ecosystem | Detected-pitch envelope export as Reaper/Audacity automation/label track | Bridges detection output into editor workflows. |
| M476 | P4 | pro-audio-ecosystem | Cross-format installer (VST3/AU/CLAP/AAX) with signed packages + manifest | Makes plugin distribution trustworthy and versioned. |
| M477 | P4 | community-social | Invite-a-bandmate onboarding pack: shareable .bandpack + printable one-pager | Drives word-of-mouth growth among bandmates. |
| M478 | P4 | monetization | Bundle on-device intelligence as opt-in 'Pro Listening' tier | Connects the ML features to a future revenue line. |
| M479 | P4 | brand-microinteractions | Seasonal accent themes auto-applied by date with manual override | Periodic freshness that invites users to return. |
| M480 | P4 | bowed-strings | Scordatura preset library per piece | Bach G-minor, Mahler, Saint-Saens Danse Macabre A-Eb. |
| M481 | P4 | keyed-free-reed | Piano 88-key sectioned tuning map A0-C8 | Visual keyboard split into bass/temperament/treble tuning sections |
| M482 | P4 | live-deep | Tuner-out passthrough: mute audio while detecting | Emulate pedalboard tuner-out by gating output during tune. |
| M483 | P4 | live-deep | Pre-show checklist: all strings green before set | Confirm every open string in tune before walking onstage. |
| M484 | P4 | live-deep | Hold-to-tune latch pins display while glancing away | Freeze last reading so tech reads after string stops. |
| M485 | P4 | kids-gamify | XP awarded per string within cents tolerance | Faster, steadier tuning grants more XP; shown as bar |
| M486 | P4 | kids-gamify | Tuning streak counter with streak-freeze token | Consecutive days tracked; earned token skips one missed day |
| M487 | P4 | kids-gamify | Color-by-string game: match strummed string to its hue | Detect played string, child taps matching colored pad |
| M488 | P4 | data-viz | Pitch constellation scatter: cents vs amplitude points | Each frame a dot; cluster tightness signals tuning stability. |
| M489 | P4 | power-user | Fully remappable hotkey editor in settings | Per-action key capture stored in localStorage, conflict detection |
| M490 | P4 | privacy-security | local CSP violation collector logging to in-app panel | report-to endpoint writes violations locally, no external reporting URI. |
| M491 | P4 | privacy-security | Threat-model doc STRIDE for mic audio and storage | Document trust boundaries, attack surface, mitigations in repo. |
| M492 | P4 | offline-storage | Last-write-wins conflict resolution with timestamp tiebreak | Per-pack updatedAt compares local vs synced, prompt on tie. |
| M493 | P4 | offline-storage | Offline pack availability badge per gallery entry | Mark which community packs are cached and usable offline. |
| M494 | P4 | theming-identity | Wallpaper-extracted palette via native OS accent API | Native egui may use a platform adapter. |
| M495 | P4 | theming-identity | Texture/material backdrop layer brushed-metal or felt | Optional subtle tiled SVG behind cards per theme. |
| M496 | P4 | notifications-engagement | Streak-at-risk nudge before midnight local time | Fire only if today's session count is zero near cutoff. |
| M497 | P4 | notifications-engagement | Weekly recap notification: strings tuned, accuracy delta | Sunday summary pulled from local IndexedDB session stats. |
| M498 | P4 | notifications-engagement | Re-engagement nudge after N days lapsed | Single gentle ping after 7-day inactivity, then escalating cooldown. |
| M499 | P4 | notifications-engagement | Per-channel opt-in: OS push vs in-app inbox | Independent toggles per notification type and delivery surface. |
| M500 | P4 | notifications-engagement | In-app notification inbox with unread badge | Persistent local list of past nudges, recaps, announcements. |
<!-- TOP500_README:END -->

</details>

## Технический долг и что сделано плохо

Полный ranked Top 500 того, что сделано плохо, неправильно, рискованно или стратегически недостроено, и исторический детальный `C#` audit находятся в одном [TOP-500-backlog.md](TOP-500-backlog.md). Текущий source of truth: [recommendation.md](recommendation.md): в первом диапазоне **102** пункта открыто/частично и **78** закрыты, независимый пост-рефакторный проход содержит **191** открытый и **23** закрытых пункта — **293 открытых и 101 закрытый всего**, разложенных по 36 SOLID/DRY-кусочкам.

Ключевые проблемы на сегодня (выборка):
- Controller/port/settings/tuning decomposition и единый web output-audio owner выполнены и защищены архитектурными/adapter tests; следующий coupling-риск — скрытые lifecycle failures и платформенная error taxonomy.
- Native egui adapter realtime-safe; web/synthetic/file реализуют единый TS `AudioInputPort`, включая sample-indexed WAV и exact PCM capture capability.
- Web WASM и egui используют `pitch-core` для primary detection; full-frame browser WASM владеет smoothing/resolution, а TS fallback проверяется теми же cents/confidence fixtures и общим smoothing trace manifest.
- Таблицы строев и note/cents primitives генерируются из canonical registry/expression sources; stale output блокируется проверкой.
- Real-WAV temporal gate, permission/device-loss recovery, HPS octave guard, фазовое уточнение периода, SNR-сетка 30/20/10 дБ и reverb-сетка RT60 0.3/0.8/1.5 с выполнены (clean MAE 2.436 цента, сетки 57/57 и 57/57); нужны benchmark/soak/visual regression tests и более широкий DSP fuzzing.
- Diagnostics, typed errors, automated a11y и release hardening ещё не закрыты.

См. также раздел "Current Problems" в [ARCHITECTURE.md](ARCHITECTURE.md).

Полный master Top 500 — в [TOP-500-backlog.md](TOP-500-backlog.md). Текущие `R#` проблемы — в [recommendation.md](recommendation.md).

Когда фиксим — обновляем [recommendation.md](recommendation.md), единый [TOP-500-backlog.md](TOP-500-backlog.md) при изменении `M#`/`C#` статуса, [ARCHITECTURE.md](ARCHITECTURE.md), этот README и, если меняется порядок работ, [PLAN.md](PLAN.md) / [RECOMMENDATIONS.md](RECOMMENDATIONS.md).
