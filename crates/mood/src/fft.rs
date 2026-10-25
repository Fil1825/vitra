//! A small radix-2 FFT, enough for the analysis's fixed frame sizes (a crate such as rustfft
//! would bring several dependencies for what is fifty lines here).

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Complex {
    pub re: f32,
    pub im: f32,
}

impl Complex {
    pub fn norm(self) -> f32 {
        self.re.hypot(self.im)
    }
}

/// An FFT plan for one size (a power of two): bit-reversal order and twiddle factors.
pub struct Fft {
    size: usize,
    reversed: Vec<usize>,
    twiddles: Vec<Complex>,
}

impl Fft {
    pub fn new(size: usize) -> Self {
        assert!(
            size.is_power_of_two() && size >= 2,
            "FFT size must be a power of two"
        );
        let bits = size.trailing_zeros();
        let reversed = (0..size)
            .map(|i| i.reverse_bits() >> (usize::BITS - bits))
            .collect();
        let twiddles = (0..size / 2)
            .map(|k| {
                let angle = -2.0 * std::f64::consts::PI * k as f64 / size as f64;
                Complex {
                    re: angle.cos() as f32,
                    im: angle.sin() as f32,
                }
            })
            .collect();
        Self {
            size,
            reversed,
            twiddles,
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// Transforms `data` (of the plan's size) in place.
    pub fn run(&self, data: &mut [Complex]) {
        assert_eq!(data.len(), self.size);
        for (i, &j) in self.reversed.iter().enumerate() {
            if i < j {
                data.swap(i, j);
            }
        }
        let mut half = 1;
        while half < self.size {
            let stride = self.size / (half * 2);
            for start in (0..self.size).step_by(half * 2) {
                for k in 0..half {
                    let w = self.twiddles[k * stride];
                    let a = data[start + k];
                    let b = data[start + k + half];
                    let t = Complex {
                        re: b.re * w.re - b.im * w.im,
                        im: b.re * w.im + b.im * w.re,
                    };
                    data[start + k] = Complex {
                        re: a.re + t.re,
                        im: a.im + t.im,
                    };
                    data[start + k + half] = Complex {
                        re: a.re - t.re,
                        im: a.im - t.im,
                    };
                }
            }
            half *= 2;
        }
    }

    /// Magnitudes of the real signal `frame` (the first `size / 2 + 1` bins).
    pub fn magnitudes(&self, frame: &[f32], out: &mut Vec<f32>) {
        let mut data: Vec<Complex> = frame.iter().map(|&re| Complex { re, im: 0.0 }).collect();
        data.resize(self.size, Complex::default());
        self.run(&mut data);
        out.clear();
        out.extend(data[..=self.size / 2].iter().map(|c| c.norm()));
    }
}

/// The Hann window of `size` samples.
pub fn hann(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| {
            let x = std::f64::consts::PI * 2.0 * i as f64 / size as f64;
            (0.5 - 0.5 * x.cos()) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The textbook O(n²) transform.
    fn dft(input: &[Complex]) -> Vec<Complex> {
        let n = input.len();
        (0..n)
            .map(|k| {
                let mut sum = Complex::default();
                for (t, x) in input.iter().enumerate() {
                    let angle = -2.0 * std::f64::consts::PI * (k * t) as f64 / n as f64;
                    let (s, c) = angle.sin_cos();
                    sum.re += x.re * c as f32 - x.im * s as f32;
                    sum.im += x.re * s as f32 + x.im * c as f32;
                }
                sum
            })
            .collect()
    }

    #[test]
    fn matches_the_plain_transform() {
        for size in [2, 8, 64, 256] {
            let input: Vec<Complex> = (0..size)
                .map(|i| Complex {
                    re: ((i * 7 % 13) as f32 - 6.0) / 6.0,
                    im: ((i * 3 % 5) as f32 - 2.0) / 4.0,
                })
                .collect();
            let mut fast = input.clone();
            Fft::new(size).run(&mut fast);
            for (a, b) in fast.iter().zip(dft(&input)) {
                assert!((a.re - b.re).abs() < 1e-3 && (a.im - b.im).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn a_sine_lands_in_its_bin() {
        let size = 1024;
        let frame: Vec<f32> = (0..size)
            .map(|i| (2.0 * std::f32::consts::PI * 32.0 * i as f32 / size as f32).sin())
            .collect();
        let mut magnitudes = Vec::new();
        Fft::new(size).magnitudes(&frame, &mut magnitudes);
        let peak = magnitudes
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i);
        assert_eq!(peak, Some(32));
        assert_eq!(magnitudes.len(), size / 2 + 1);
    }
}
