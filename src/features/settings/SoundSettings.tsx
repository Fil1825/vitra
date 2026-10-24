import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { useTranslation } from 'react-i18next';

import { SegmentedControl } from '../../components/ui/SegmentedControl.tsx';
import { Slider } from '../../components/ui/Slider.tsx';
import { Switch } from '../../components/ui/Switch.tsx';
import { player } from '../../lib/player.ts';
import {
  CROSSFADE_MAX,
  EQ_BANDS,
  EQ_GAIN_MAX,
  type Equalizer,
  type Normalization,
  type SoundSettings as Sound,
} from '../../lib/settings.ts';
import { useSettings } from '../../stores/settings.ts';
import {
  BUILTIN_IDS,
  bandLabel,
  gainLabel,
  isBuiltin,
  presetCurve,
  userPresetId,
  withGain,
} from '../sound/presets.ts';
import { Group, Row } from './Row.tsx';
import styles from './SettingsPanel.module.css';

const report = (what: string) => (err: unknown) => console.error(`${what} failed`, err);

function useSound() {
  const sound = useSettings((s) => s.settings.sound);
  const update = useSettings((s) => s.update);
  const change = (next: (sound: Sound) => Sound) =>
    void update((current) => ({ ...current, sound: next(current.sound) })).catch(
      report('Saving the sound settings'),
    );
  return [sound, change] as const;
}

/** A band's (or the preamp's) slider, standing up, with its gain above and name below. */
function EqSlider({
  label,
  name,
  value,
  onMove,
  onDone,
}: {
  label: string;
  name: string;
  value: number;
  onMove(value: number): void;
  onDone(value: number): void;
}) {
  return (
    <div className={styles.eqBand}>
      <span className={styles.eqGain}>{gainLabel(value)}</span>
      <div className={styles.eqTrack}>
        <Slider
          className={styles.eqSlider}
          label={label}
          valueText={`${gainLabel(value)} dB`}
          min={-EQ_GAIN_MAX}
          max={EQ_GAIN_MAX}
          step={0.5}
          value={value}
          onChange={onMove}
          onCommit={onDone}
        />
      </div>
      <span className={styles.eqName}>{name}</span>
    </div>
  );
}

function EqualizerEditor() {
  const { t } = useTranslation();
  const [sound, change] = useSound();
  // The curve being dragged: heard at once, saved when the slider is let go.
  const [draft, setDraft] = useState<Equalizer | null>(null);
  const [naming, setNaming] = useState<string | null>(null);
  const shown = draft ?? sound.equalizer;

  // Previews go out at most once a frame, the newest one.
  const pending = useRef<Equalizer | null>(null);
  const frame = useRef(0);
  useEffect(() => () => cancelAnimationFrame(frame.current), []);
  const preview = (equalizer: Equalizer) => {
    pending.current = equalizer;
    if (frame.current) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      if (pending.current) {
        player.previewEqualizer(pending.current).catch(report('Previewing the equalizer'));
      }
    });
  };

  const setEqualizer = (equalizer: Equalizer) => {
    setDraft(null);
    change((current) => ({ ...current, equalizer }));
  };
  // Moving a band turns the equalizer on, as in other players.
  const moved = (band: number, value: number) => ({
    ...withGain(shown, band, value),
    enabled: true,
  });

  const choosePreset = (id: string) => {
    const curve = presetCurve(id, sound.presets);
    if (curve) setEqualizer({ ...shown, ...curve, preset: id, enabled: true });
  };
  const savePreset = (name: string) => {
    const trimmed = name.trim();
    if (!trimmed) return;
    setNaming(null);
    change((current) => ({
      ...current,
      presets: [
        ...current.presets.filter((preset) => preset.name !== trimmed),
        { name: trimmed, gains: shown.gains, preamp: shown.preamp },
      ],
      equalizer: { ...shown, preset: userPresetId(trimmed) },
    }));
  };
  const userPreset = shown.preset.startsWith('user:') ? shown.preset.slice(5) : null;
  const deletePreset = (name: string) =>
    change((current) => ({
      ...current,
      presets: current.presets.filter((preset) => preset.name !== name),
      equalizer: { ...current.equalizer, preset: '' },
    }));

  return (
    <>
      <Row label={t('sound.equalizer')} hint={t('sound.equalizerHint')} layout="inline">
        <Switch
          label={t('sound.equalizer')}
          checked={shown.enabled}
          onChange={(enabled) => setEqualizer({ ...shown, enabled })}
        />
      </Row>

      <label className={styles.row}>
        <span className={styles.rowText}>
          <span>{t('sound.preset')}</span>
        </span>
        <select
          className={styles.textInput}
          value={shown.preset || 'custom'}
          onChange={(event) => choosePreset(event.target.value)}
        >
          {!shown.preset && (
            <option value="custom" disabled>
              {t('sound.custom')}
            </option>
          )}
          {BUILTIN_IDS.map((id) => (
            <option key={id} value={id}>
              {t(`sound.presetNames.${id}`)}
            </option>
          ))}
          {sound.presets.length > 0 && (
            <optgroup label={t('sound.yourPresets')}>
              {sound.presets.map((preset) => (
                <option key={preset.name} value={userPresetId(preset.name)}>
                  {preset.name}
                </option>
              ))}
            </optgroup>
          )}
        </select>
      </label>

      <div
        className={styles.eqBoard}
        data-off={!shown.enabled || undefined}
        style={{ '--_bands': EQ_BANDS.length } as CSSProperties}
      >
        <EqSlider
          label={t('sound.preamp')}
          name={t('sound.preampShort')}
          value={shown.preamp}
          onMove={(value) => {
            const next = moved(-1, value);
            setDraft(next);
            preview(next);
          }}
          onDone={(value) => setEqualizer(moved(-1, value))}
        />
        <span className={styles.eqDivider} aria-hidden="true" />
        {EQ_BANDS.map((hz, band) => (
          <EqSlider
            key={hz}
            label={t('sound.band', { band: bandLabel(hz) })}
            name={bandLabel(hz)}
            value={shown.gains[band] ?? 0}
            onMove={(value) => {
              const next = moved(band, value);
              setDraft(next);
              preview(next);
            }}
            onDone={(value) => setEqualizer(moved(band, value))}
          />
        ))}
      </div>

      <div className={styles.buttonRow}>
        <button type="button" className={styles.textButton} onClick={() => choosePreset('flat')}>
          {t('sound.reset')}
        </button>
        {naming === null ? (
          <button type="button" className={styles.textButton} onClick={() => setNaming('')}>
            {t('sound.savePreset')}
          </button>
        ) : (
          <form
            className={styles.inlineForm}
            onSubmit={(event) => {
              event.preventDefault();
              savePreset(naming);
            }}
          >
            <input
              className={styles.textInput}
              // Shown because the user asked to name the preset.
              autoFocus
              aria-label={t('sound.presetName')}
              placeholder={t('sound.presetName')}
              value={naming}
              maxLength={60}
              onChange={(event) => setNaming(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Escape') {
                  event.preventDefault();
                  setNaming(null);
                }
              }}
            />
            <button type="submit" className={styles.textButton} disabled={!naming.trim()}>
              {t('sound.save')}
            </button>
          </form>
        )}
        {userPreset !== null && !isBuiltin(shown.preset) && (
          <button
            type="button"
            className={styles.textButton}
            onClick={() => deletePreset(userPreset)}
          >
            {t('sound.deletePreset')}
          </button>
        )}
      </div>
    </>
  );
}

/** Settings → Sound: the equalizer, even loudness, crossfade. */
export function SoundSettings() {
  const { t } = useTranslation();
  const [sound, change] = useSound();
  const [crossfade, setCrossfade] = useState<number | null>(null);
  const seconds = crossfade ?? sound.crossfade;

  return (
    <>
      <Group title={t('sound.equalizer')}>
        <EqualizerEditor />
      </Group>

      <Group title={t('sound.volume')}>
        <Row label={t('sound.normalization')} hint={t('sound.normalizationHint')}>
          <SegmentedControl<Normalization>
            label={t('sound.normalization')}
            value={sound.normalization}
            options={[
              { value: 'off', label: t('sound.normalizations.off') },
              { value: 'track', label: t('sound.normalizations.track') },
              { value: 'album', label: t('sound.normalizations.album') },
            ]}
            onChange={(normalization) => change((current) => ({ ...current, normalization }))}
          />
        </Row>
      </Group>

      <Group title={t('sound.transitions')}>
        <Row label={t('sound.crossfade')} hint={t('sound.crossfadeHint')}>
          <div className={styles.sliderRow}>
            <Slider
              label={t('sound.crossfade')}
              valueText={
                seconds === 0 ? t('sound.gapless') : t('sound.seconds', { count: seconds })
              }
              min={0}
              max={CROSSFADE_MAX}
              step={0.5}
              value={seconds}
              onChange={setCrossfade}
              onCommit={(value) => {
                setCrossfade(null);
                change((current) => ({ ...current, crossfade: value }));
              }}
            />
            <span className={styles.sliderValue}>
              {seconds === 0 ? t('sound.gapless') : t('sound.seconds', { count: seconds })}
            </span>
          </div>
        </Row>
      </Group>
    </>
  );
}
