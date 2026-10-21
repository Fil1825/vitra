import { useId, useRef, type HTMLAttributes, type RefObject } from 'react';

import { useElementSize } from '../../lib/useElementSize.ts';
import { useAppearance } from '../../stores/appearance.ts';
import { useGlassSlot } from './budget.ts';
import styles from './Glass.module.css';
import { MAX_DISPLACEMENT_PX, displacementMaps } from './refraction.ts';
import { glassStyle, type GlassOptions } from './style.ts';

export interface GlassProps extends GlassOptions, HTMLAttributes<HTMLElement> {
  as?: 'div' | 'section' | 'aside' | 'header' | 'footer' | 'nav' | 'main';
  /** Higher-priority layers keep their blur when too many glass surfaces are visible. */
  priority?: number;
}

/** A translucent "liquid glass" surface: blur, tint, specular rim and (where possible) refraction. */
export function Glass({
  as: Tag = 'div',
  priority = 0,
  blur,
  saturation,
  tint,
  opacity,
  radius,
  border,
  highlight,
  refraction,
  className,
  style,
  children,
  ...rest
}: GlassProps) {
  const id = useId();
  const ref = useRef<HTMLElement>(null);
  const canBlur = useGlassSlot(id, priority);
  const effects = useAppearance((s) => s.effects);
  const themeRefraction = useAppearance((s) => s.theme.glass.refraction);

  const strength = refraction ?? themeRefraction;
  const refracting = canBlur && effects.refraction && strength > 0;
  const size = useElementSize(ref, refracting);
  const filterId = `vitra-refraction-${id.replace(/[^\w-]/g, '')}`;

  const vars = glassStyle({ blur, saturation, tint, opacity, radius, border, highlight });
  if (refracting && size) {
    Object.assign(vars, { '--glass-refraction-filter': `url(#${filterId})` });
  }

  return (
    <Tag
      // Every allowed tag is a plain HTMLElement; TS can't tie a union of tags to one ref type.
      ref={ref as RefObject<HTMLDivElement | null>}
      className={className ? `${styles.glass} ${className}` : styles.glass}
      data-blur={canBlur ? 'on' : 'off'}
      data-refraction={refracting && size ? 'on' : 'off'}
      style={{ ...vars, ...style }}
      {...rest}
    >
      {refracting && size && (
        <RefractionFilter
          id={filterId}
          width={size.width}
          height={size.height}
          strength={strength}
        />
      )}
      {children}
    </Tag>
  );
}

function RefractionFilter({
  id,
  width,
  height,
  strength,
}: {
  id: string;
  width: number;
  height: number;
  strength: number;
}) {
  const maps = displacementMaps(width, height);
  return (
    <svg className={styles.filterHost} aria-hidden="true" focusable="false">
      <filter
        id={id}
        x="0"
        y="0"
        width={width}
        height={height}
        filterUnits="userSpaceOnUse"
        colorInterpolationFilters="sRGB"
      >
        <feImage
          href={maps.x}
          width={width}
          height={height}
          preserveAspectRatio="none"
          result="x"
        />
        <feImage
          href={maps.y}
          width={width}
          height={height}
          preserveAspectRatio="none"
          result="y"
        />
        {/* Red carries the horizontal shift, green the vertical one: add them into one map. */}
        <feComposite
          in="x"
          in2="y"
          operator="arithmetic"
          k1={0}
          k2={1}
          k3={1}
          k4={0}
          result="map"
        />
        <feDisplacementMap
          in="SourceGraphic"
          in2="map"
          scale={Math.round(strength * MAX_DISPLACEMENT_PX)}
          xChannelSelector="R"
          yChannelSelector="G"
        />
      </filter>
    </svg>
  );
}
