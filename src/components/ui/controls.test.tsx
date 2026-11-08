import { fireEvent, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { renderUi } from '../../test/render.ts';
import { IconButton } from './IconButton.tsx';
import { SegmentedControl } from './SegmentedControl.tsx';
import { Slider } from './Slider.tsx';
import { Switch } from './Switch.tsx';

describe('IconButton', () => {
  it('exposes its label and runs the click', async () => {
    const user = userEvent.setup();
    const onClick = vi.fn();
    await renderUi(
      <IconButton label="Play" onClick={onClick}>
        <span>icon</span>
      </IconButton>,
    );

    const button = screen.getByRole('button', { name: 'Play' });
    expect(button).toHaveAttribute('type', 'button');
    expect(button).toHaveAttribute('title', 'Play');
    await user.click(button);
    expect(onClick).toHaveBeenCalledOnce();
  });

  it('does not run a click while disabled', async () => {
    const user = userEvent.setup();
    const onClick = vi.fn();
    await renderUi(
      <IconButton label="Next" disabled onClick={onClick}>
        <span>icon</span>
      </IconButton>,
    );

    await user.click(screen.getByRole('button', { name: 'Next' }));
    expect(onClick).not.toHaveBeenCalled();
  });
});

describe('Switch', () => {
  it('toggles the opposite of its current value', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    await renderUi(<Switch label="Vinyl" checked={false} onChange={onChange} />);

    const toggle = screen.getByRole('switch', { name: 'Vinyl' });
    expect(toggle).toHaveAttribute('aria-checked', 'false');
    await user.click(toggle);
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it('ignores a click while disabled', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    await renderUi(<Switch label="Vinyl" checked onChange={onChange} disabled />);

    await user.click(screen.getByRole('switch', { name: 'Vinyl' }));
    expect(onChange).not.toHaveBeenCalled();
  });
});

describe('SegmentedControl', () => {
  it('selects the option that was pressed', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    await renderUi(
      <SegmentedControl
        label="Appearance"
        value="light"
        options={[
          { value: 'light', label: 'Light' },
          { value: 'dark', label: 'Dark' },
        ]}
        onChange={onChange}
      />,
    );

    expect(screen.getByRole('radio', { name: 'Light' })).toHaveAttribute('aria-checked', 'true');
    await user.click(screen.getByRole('radio', { name: 'Dark' }));
    expect(onChange).toHaveBeenCalledWith('dark');
  });

  it('leaves a disabled option alone', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    await renderUi(
      <SegmentedControl
        label="Appearance"
        value="light"
        options={[
          { value: 'light', label: 'Light' },
          { value: 'dark', label: 'Dark', disabled: true },
        ]}
        onChange={onChange}
      />,
    );

    await user.click(screen.getByRole('radio', { name: 'Dark' }));
    expect(onChange).not.toHaveBeenCalled();
  });
});

describe('Slider', () => {
  it('reports every move and the value it was released on', async () => {
    const onChange = vi.fn();
    const onCommit = vi.fn();

    function Volume() {
      const [value, setValue] = useState(20);
      return (
        <Slider
          label="Volume"
          value={value}
          max={100}
          onChange={(next) => {
            onChange(next);
            setValue(next);
          }}
          onCommit={onCommit}
        />
      );
    }

    await renderUi(<Volume />);
    const slider = screen.getByRole('slider', { name: 'Volume' });
    expect(slider).toHaveValue('20');
    fireEvent.change(slider, { target: { value: '40' } });
    expect(onChange).toHaveBeenCalledWith(40);
    fireEvent.pointerUp(screen.getByRole('slider', { name: 'Volume' }));
    expect(onCommit).toHaveBeenCalledWith(40);
  });
});
