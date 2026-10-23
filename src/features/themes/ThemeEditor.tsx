import { open } from '@tauri-apps/plugin-dialog';
import { ImagePlus, Minus, Plus, RotateCcw, Save, Share, Type, X } from 'lucide-react';
import { motion } from 'motion/react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../../components/Glass/index.ts';
import { IconButton } from '../../components/ui/IconButton.tsx';
import { SegmentedControl } from '../../components/ui/SegmentedControl.tsx';
import { Switch } from '../../components/ui/Switch.tsx';
import { useSettings } from '../../stores/settings.ts';
import type { ThemeDraft } from '../../stores/themeEditor.ts';
import { percent } from '../../theme/color.ts';
import { sanitizeCss } from '../../theme/css.ts';
import { FONT_EXTENSIONS, IMAGE_EXTENSIONS, type Theme } from '../../theme/schema.ts';
import { exportTheme } from './actions.ts';
import {
  addDraftAsset,
  closeThemeEditor,
  isDirty,
  revertDraft,
  saveDraft,
  updateDraft,
  updateTheme,
} from './editor.ts';
import { ColorField, Field, NumberField, Section } from './fields.tsx';
import styles from './ThemeEditor.module.css';

const report = (what: string) => (err: unknown) => console.error(`${what} failed`, err);

/** Font stacks offered by name; anything else is typed. */
const FONT_PRESETS = {
  system: "system-ui, -apple-system, 'Segoe UI', Roboto, 'Helvetica Neue', sans-serif",
  rounded: "ui-rounded, 'SF Pro Rounded', system-ui, sans-serif",
  serif: "ui-serif, 'New York', Georgia, 'Times New Roman', serif",
  mono: "ui-monospace, 'SF Mono', Menlo, Consolas, monospace",
} as const;
type FontPreset = keyof typeof FONT_PRESETS | 'custom';

const FONT_STACK = /^[\w\s,'"-]+$/;

type Colors = Theme['colors'];
type GlassValues = Theme['glass'];
type Background = Theme['background'];

/**
 * The theme editor: a panel over the window, whose every part shows the draft as it changes
 * (the appearance controller applies the draft in place of the chosen theme). Built-in themes
 * are edited as copies, so the draft is always an installed theme.
 */
export function ThemeEditor({ draft }: { draft: ThemeDraft }) {
  const { t } = useTranslation();
  const theme = draft.theme;
  const dirty = isDirty(draft);

  const setColors = (change: Partial<Colors>) =>
    updateTheme((th) => ({ ...th, colors: { ...th.colors, ...change } }));
  const setGlass = (change: Partial<GlassValues>) =>
    updateTheme((th) => ({ ...th, glass: { ...th.glass, ...change } }));
  const setBackground = (change: Partial<Background>) =>
    updateTheme((th) => ({ ...th, background: { ...th.background, ...change } }));

  const colorFields: [keyof Colors, string][] = [
    ['background', t('themeEditor.background')],
    ['text', t('themeEditor.text')],
    ['textSecondary', t('themeEditor.textSecondary')],
    ['textTertiary', t('themeEditor.textTertiary')],
    ['accent', t('themeEditor.accent')],
    ['accentContrast', t('themeEditor.accentContrast')],
    ['separator', t('themeEditor.separator')],
    ['danger', t('themeEditor.danger')],
  ];

  return (
    // Only transform animates: opacity on an ancestor would cut the glass off from what it blurs.
    <motion.div
      className={styles.wrap}
      initial={{ x: '110%' }}
      animate={{ x: 0 }}
      exit={{ x: '110%' }}
      transition={{ type: 'spring', stiffness: 380, damping: 38 }}
    >
      <Glass as="aside" className={styles.panel} aria-label={t('themeEditor.title')} priority={5}>
        <header className={styles.header}>
          <div className={styles.headerText}>
            <h2 className={styles.title}>{t('themeEditor.title')}</h2>
            <span className={styles.subtitle}>
              {theme.name}
              {dirty && ' •'}
            </span>
          </div>
          <IconButton
            label={t('themeEditor.close')}
            onClick={() => void closeThemeEditor().catch(report('Closing the editor'))}
          >
            <X size={18} />
          </IconButton>
        </header>

        <div className={styles.content}>
          <Section title={t('themeEditor.general')} open>
            <Field label={t('themeEditor.name')}>
              <input
                className={styles.text}
                value={theme.name}
                maxLength={64}
                onChange={(event) =>
                  updateTheme((th) => ({ ...th, name: event.currentTarget.value }))
                }
              />
            </Field>
            <Field label={t('themeEditor.author')}>
              <input
                className={styles.text}
                value={theme.author ?? ''}
                maxLength={100}
                onChange={(event) => {
                  const author = event.currentTarget.value;
                  updateTheme((th) => ({ ...th, author: author || undefined }));
                }}
              />
            </Field>
            <Field label={t('themeEditor.appearance')}>
              <SegmentedControl
                label={t('themeEditor.appearance')}
                value={theme.appearance}
                options={[
                  { value: 'light', label: t('themeEditor.light') },
                  { value: 'dark', label: t('themeEditor.dark') },
                ]}
                onChange={(appearance) => updateTheme((th) => ({ ...th, appearance }))}
              />
            </Field>
          </Section>

          <Section title={t('themeEditor.colors')} open>
            {colorFields.map(([key, label]) => (
              <ColorField
                key={key}
                label={label}
                value={theme.colors[key]}
                onChange={(value) => setColors({ [key]: value })}
              />
            ))}
            <Field label={t('themeEditor.accentFromArtwork')}>
              <Switch
                label={t('themeEditor.accentFromArtwork')}
                checked={theme.accentFromArtwork}
                onChange={(accentFromArtwork) =>
                  updateTheme((th) => ({ ...th, accentFromArtwork }))
                }
              />
            </Field>
          </Section>

          <Section title={t('themeEditor.glass')}>
            <NumberField
              label={t('themeEditor.blur')}
              value={theme.glass.blur}
              min={0}
              max={80}
              unit=" px"
              onChange={(blur) => setGlass({ blur })}
            />
            <NumberField
              label={t('themeEditor.saturation')}
              value={theme.glass.saturation}
              min={0}
              max={3}
              step={0.05}
              format={(v) => `${v.toFixed(2)}×`}
              onChange={(saturation) => setGlass({ saturation })}
            />
            <ColorField
              label={t('themeEditor.tint')}
              value={theme.glass.tint}
              onChange={(tint) => setGlass({ tint })}
            />
            <NumberField
              label={t('themeEditor.tintOpacity')}
              value={theme.glass.opacity}
              min={0}
              max={1}
              step={0.01}
              format={percent}
              onChange={(opacity) => setGlass({ opacity })}
            />
            <ColorField
              label={t('themeEditor.border')}
              value={theme.glass.border}
              onChange={(border) => setGlass({ border })}
            />
            <NumberField
              label={t('themeEditor.borderWidth')}
              value={theme.glass.borderWidth}
              min={0}
              max={4}
              step={0.5}
              unit=" px"
              onChange={(borderWidth) => setGlass({ borderWidth })}
            />
            <NumberField
              label={t('themeEditor.highlight')}
              value={theme.glass.highlight}
              min={0}
              max={1}
              step={0.01}
              format={percent}
              onChange={(highlight) => setGlass({ highlight })}
            />
            <ColorField
              label={t('themeEditor.highlightColor')}
              value={theme.glass.highlightColor}
              onChange={(highlightColor) => setGlass({ highlightColor })}
            />
            <NumberField
              label={t('themeEditor.refraction')}
              value={theme.glass.refraction}
              min={0}
              max={1}
              step={0.01}
              format={percent}
              onChange={(refraction) => setGlass({ refraction })}
            />
            <p className={styles.hint}>{t('themeEditor.refractionHint')}</p>
            <ColorField
              label={t('themeEditor.shadow')}
              value={theme.glass.shadow}
              onChange={(shadow) => setGlass({ shadow })}
            />
            <NumberField
              label={t('themeEditor.shadowBlur')}
              value={theme.glass.shadowBlur}
              min={0}
              max={120}
              unit=" px"
              onChange={(shadowBlur) => setGlass({ shadowBlur })}
            />
          </Section>

          <Section title={t('themeEditor.typography')}>
            <FontFields theme={theme} />
          </Section>

          <Section title={t('themeEditor.shape')}>
            {(['small', 'medium', 'large'] as const).map((size) => (
              <NumberField
                key={size}
                label={t(`themeEditor.radius.${size}`)}
                value={theme.radius[size]}
                min={0}
                max={48}
                unit=" px"
                onChange={(value) =>
                  updateTheme((th) => ({ ...th, radius: { ...th.radius, [size]: value } }))
                }
              />
            ))}
            <NumberField
              label={t('themeEditor.spacing')}
              value={theme.spacing.unit}
              min={2}
              max={8}
              step={0.5}
              unit=" px"
              onChange={(unit) => updateTheme((th) => ({ ...th, spacing: { unit } }))}
            />
          </Section>

          <Section title={t('themeEditor.backdrop')}>
            <BackgroundFields theme={theme} setBackground={setBackground} />
          </Section>

          <Section title={t('themeEditor.motion')}>
            <NumberField
              label={t('themeEditor.duration')}
              value={theme.motion.duration}
              min={0}
              max={2000}
              step={10}
              unit=" ms"
              onChange={(duration) =>
                updateTheme((th) => ({ ...th, motion: { ...th.motion, duration } }))
              }
            />
            <Field label={t('themeEditor.easing')}>
              <SegmentedControl
                label={t('themeEditor.easing')}
                value={theme.motion.easing}
                options={[
                  { value: 'standard', label: t('themeEditor.easings.standard') },
                  { value: 'emphasized', label: t('themeEditor.easings.emphasized') },
                  { value: 'linear', label: t('themeEditor.easings.linear') },
                ]}
                onChange={(easing) =>
                  updateTheme((th) => ({ ...th, motion: { ...th.motion, easing } }))
                }
              />
            </Field>
            <NumberField
              label={t('themeEditor.backgroundSpeed')}
              value={theme.motion.backgroundSpeed}
              min={0}
              max={4}
              step={0.1}
              format={(v) => `${v.toFixed(1)}×`}
              onChange={(backgroundSpeed) =>
                updateTheme((th) => ({ ...th, motion: { ...th.motion, backgroundSpeed } }))
              }
            />
          </Section>

          <Section title={t('themeEditor.layout')}>
            <LayoutFields theme={theme} />
          </Section>

          <Section title={t('themeEditor.css')}>
            <CssField draft={draft} />
          </Section>
        </div>

        <footer className={styles.footer}>
          <button type="button" className={styles.button} disabled={!dirty} onClick={revertDraft}>
            <RotateCcw size={14} aria-hidden="true" />
            {t('themeEditor.revert')}
          </button>
          <button
            type="button"
            className={styles.button}
            onClick={() => void exportTheme(theme, draft.css).catch(report('Exporting'))}
          >
            <Share size={14} aria-hidden="true" />
            {t('themeEditor.export')}
          </button>
          <button
            type="button"
            className={`${styles.button} ${styles.primary}`}
            disabled={!dirty}
            onClick={() => void saveDraft().catch(report('Saving the theme'))}
          >
            <Save size={14} aria-hidden="true" />
            {t('themeEditor.save')}
          </button>
        </footer>
      </Glass>
    </motion.div>
  );
}

function FontFields({ theme }: { theme: Theme }) {
  const { t } = useTranslation();
  const [typed, setTyped] = useState<string | null>(null);
  const preset: FontPreset =
    (Object.entries(FONT_PRESETS).find(
      ([, stack]) => stack === theme.typography.fontFamily,
    )?.[0] as FontPreset | undefined) ?? 'custom';
  const setFamily = (fontFamily: string) =>
    updateTheme((th) => ({ ...th, typography: { ...th.typography, fontFamily } }));

  const addFont = async () => {
    const picked = await open({
      filters: [{ name: t('themeEditor.fontFilter'), extensions: [...FONT_EXTENSIONS] }],
    });
    if (typeof picked !== 'string') return;
    const src = await addDraftAsset(picked);
    if (!src) return;
    const stem = src.slice('assets/'.length).replace(/\.[^.]+$/, '');
    const family =
      stem
        .replace(/[^\w -]+/g, ' ')
        .trim()
        .slice(0, 64) || 'Theme Font';
    updateTheme((th) => ({
      ...th,
      typography: {
        ...th.typography,
        fonts: [...(th.typography.fonts ?? []), { family, src: src }].slice(0, 8),
        fontFamily: `'${family}', system-ui, sans-serif`,
      },
    }));
  };

  return (
    <>
      <Field label={t('themeEditor.font')}>
        <select
          className={styles.select}
          value={preset}
          onChange={(event) => {
            const value = event.currentTarget.value as FontPreset;
            if (value !== 'custom') setFamily(FONT_PRESETS[value]);
          }}
        >
          {(['system', 'rounded', 'serif', 'mono', 'custom'] as const).map((key) => (
            <option key={key} value={key}>
              {t(`themeEditor.fontPresets.${key}`)}
            </option>
          ))}
        </select>
      </Field>
      <input
        className={styles.text}
        aria-label={t('themeEditor.customFont')}
        placeholder={t('themeEditor.customFont')}
        spellCheck={false}
        value={typed ?? theme.typography.fontFamily}
        aria-invalid={typed !== null && !FONT_STACK.test(typed)}
        onChange={(event) => {
          const text = event.currentTarget.value;
          setTyped(text);
          if (text.trim() && FONT_STACK.test(text)) setFamily(text);
        }}
        onBlur={() => setTyped(null)}
      />
      {(theme.typography.fonts ?? []).map((font) => (
        <div key={font.src} className={styles.asset}>
          <Type size={14} aria-hidden="true" />
          <span className={styles.assetName}>{font.family}</span>
          <IconButton
            label={t('themeEditor.removeFont')}
            onClick={() =>
              updateTheme((th) => {
                const fonts = th.typography.fonts?.filter((f) => f.src !== font.src);
                return {
                  ...th,
                  typography: { ...th.typography, fonts: fonts?.length ? fonts : undefined },
                };
              })
            }
          >
            <Minus size={14} />
          </IconButton>
        </div>
      ))}
      <button
        type="button"
        className={styles.button}
        disabled={(theme.typography.fonts?.length ?? 0) >= 8}
        onClick={() => void addFont().catch(report('Adding a font'))}
      >
        <Plus size={14} aria-hidden="true" />
        {t('themeEditor.addFont')}
      </button>
      <NumberField
        label={t('themeEditor.fontSize')}
        value={theme.typography.fontSize}
        min={11}
        max={20}
        step={0.5}
        unit=" px"
        onChange={(fontSize) =>
          updateTheme((th) => ({ ...th, typography: { ...th.typography, fontSize } }))
        }
      />
    </>
  );
}

function BackgroundFields({
  theme,
  setBackground,
}: {
  theme: Theme;
  setBackground(change: Partial<Background>): void;
}) {
  const { t } = useTranslation();
  const { background } = theme;
  const gradient = background.gradient;

  const choosePicture = async () => {
    const picked = await open({
      filters: [{ name: t('themeEditor.imageFilter'), extensions: [...IMAGE_EXTENSIONS] }],
    });
    if (typeof picked !== 'string') return;
    const src = await addDraftAsset(picked);
    if (src) {
      setBackground({
        image: {
          src: src,
          opacity: background.image?.opacity ?? 0.6,
          fit: background.image?.fit ?? 'cover',
        },
      });
    }
  };

  return (
    <>
      <NumberField
        label={t('themeEditor.baseOpacity')}
        value={background.baseOpacity}
        min={0}
        max={1}
        step={0.01}
        format={percent}
        onChange={(baseOpacity) => setBackground({ baseOpacity })}
      />
      {gradient.map((color, index) => (
        <div key={index} className={styles.gradientRow}>
          <ColorField
            label={t('themeEditor.gradientColor', { n: index + 1 })}
            value={color}
            onChange={(value) =>
              setBackground({ gradient: gradient.map((c, i) => (i === index ? value : c)) })
            }
          />
          <IconButton
            label={t('themeEditor.removeColor')}
            disabled={gradient.length <= 2}
            onClick={() => setBackground({ gradient: gradient.filter((_, i) => i !== index) })}
          >
            <Minus size={14} />
          </IconButton>
        </div>
      ))}
      <button
        type="button"
        className={styles.button}
        disabled={gradient.length >= 5}
        onClick={() => setBackground({ gradient: [...gradient, gradient.at(-1) ?? '#808080'] })}
      >
        <Plus size={14} aria-hidden="true" />
        {t('themeEditor.addColor')}
      </button>
      <NumberField
        label={t('themeEditor.gradientOpacity')}
        value={background.gradientOpacity}
        min={0}
        max={1}
        step={0.01}
        format={percent}
        onChange={(gradientOpacity) => setBackground({ gradientOpacity })}
      />
      <Field label={t('themeEditor.useArtwork')}>
        <Switch
          label={t('themeEditor.useArtwork')}
          checked={background.useArtwork}
          onChange={(useArtwork) => setBackground({ useArtwork })}
        />
      </Field>
      <NumberField
        label={t('themeEditor.artworkOpacity')}
        value={background.artworkOpacity}
        min={0}
        max={1}
        step={0.01}
        format={percent}
        onChange={(artworkOpacity) => setBackground({ artworkOpacity })}
      />
      <div className={styles.buttonRow}>
        <button
          type="button"
          className={styles.button}
          onClick={() => void choosePicture().catch(report('Choosing a picture'))}
        >
          <ImagePlus size={14} aria-hidden="true" />
          {t('themeEditor.choosePicture')}
        </button>
        {background.image && (
          <button
            type="button"
            className={styles.button}
            onClick={() => setBackground({ image: undefined })}
          >
            <Minus size={14} aria-hidden="true" />
            {t('themeEditor.removePicture')}
          </button>
        )}
      </div>
      {background.image && (
        <>
          <NumberField
            label={t('themeEditor.pictureOpacity')}
            value={background.image.opacity}
            min={0}
            max={1}
            step={0.01}
            format={percent}
            onChange={(opacity) =>
              background.image && setBackground({ image: { ...background.image, opacity } })
            }
          />
          <Field label={t('themeEditor.fit')}>
            <SegmentedControl
              label={t('themeEditor.fit')}
              value={background.image.fit}
              options={[
                { value: 'cover', label: t('themeEditor.fits.cover') },
                { value: 'contain', label: t('themeEditor.fits.contain') },
                { value: 'tile', label: t('themeEditor.fits.tile') },
              ]}
              onChange={(fit) =>
                background.image && setBackground({ image: { ...background.image, fit } })
              }
            />
          </Field>
        </>
      )}
    </>
  );
}

function LayoutFields({ theme }: { theme: Theme }) {
  const { t } = useTranslation();
  const layout = useSettings((s) => s.settings.layout);
  return (
    <>
      <Field label={t('themeEditor.includeLayout')}>
        <Switch
          label={t('themeEditor.includeLayout')}
          checked={theme.layout !== undefined}
          onChange={(on) => updateTheme((th) => ({ ...th, layout: on ? layout : undefined }))}
        />
      </Field>
      <p className={styles.hint}>{t('themeEditor.includeLayoutHint')}</p>
      {theme.layout && JSON.stringify(theme.layout) !== JSON.stringify(layout) && (
        <button
          type="button"
          className={styles.button}
          onClick={() => updateTheme((th) => ({ ...th, layout }))}
        >
          {t('themeEditor.updateLayout')}
        </button>
      )}
    </>
  );
}

function CssField({ draft }: { draft: ThemeDraft }) {
  const { t } = useTranslation();
  const removed = useMemo(
    () =>
      draft.css
        ? sanitizeCss(draft.css, (path) =>
            draft.assets.includes(path.slice('assets/'.length)) ? path : null,
          ).removed
        : [],
    [draft.css, draft.assets],
  );
  return (
    <>
      <p className={styles.hint}>{t('themeEditor.cssHint')}</p>
      <textarea
        className={styles.css}
        aria-label={t('themeEditor.css')}
        spellCheck={false}
        value={draft.css ?? ''}
        placeholder={'[data-part="controls"] {\n  --glass-radius: 999px;\n}'}
        onChange={(event) => {
          const css = event.currentTarget.value;
          updateDraft(() => ({ css }));
        }}
      />
      {removed.length > 0 && (
        <p className={styles.warning} role="status">
          {t('themeEditor.cssRemoved', { list: removed.slice(0, 4).join('; ') })}
        </p>
      )}
    </>
  );
}
