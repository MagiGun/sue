//! Physical quantities and units. Engineering, CAD, BIM, HVAC, plumbing,
//! electrical, print layout and audio all measure things; a value carries
//! its unit, converts exactly between units of one dimension, and refuses to
//! mix dimensions (a length is never silently added to a pressure).

use std::fmt;
use std::str::FromStr;

/// What a quantity measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Dimension {
    /// A plain number (ratios, percentages, counts).
    None,
    /// Length.
    Length,
    /// Area.
    Area,
    /// Volume.
    Volume,
    /// Plane angle.
    Angle,
    /// Time.
    Time,
    /// Frequency.
    Frequency,
    /// Mass.
    Mass,
    /// Force.
    Force,
    /// Pressure and stress.
    Pressure,
    /// Energy and work.
    Energy,
    /// Power (and heat flow).
    Power,
    /// Temperature.
    Temperature,
    /// Velocity.
    Velocity,
    /// Volumetric flow (air, water).
    FlowRate,
    /// Electric potential.
    Voltage,
    /// Electric current.
    Current,
    /// Electric resistance.
    Resistance,
    /// Capacitance.
    Capacitance,
    /// Inductance.
    Inductance,
    /// Luminous intensity.
    LuminousIntensity,
    /// Digital information.
    Data,
    /// Sound and signal level (logarithmic).
    Level,
}

/// A unit: its symbol, its dimension, and how to reach the SI base unit
/// (`si = value * scale + offset`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unit {
    /// Symbol, e.g. `mm`.
    pub symbol: &'static str,
    /// Name, e.g. `millimetre`.
    pub name: &'static str,
    /// What it measures.
    pub dimension: Dimension,
    /// Multiplier to the SI unit.
    pub scale: f64,
    /// Offset added after scaling (temperatures only).
    pub offset: f64,
}

const fn unit(symbol: &'static str, name: &'static str, dimension: Dimension, scale: f64) -> Unit {
    Unit {
        symbol,
        name,
        dimension,
        scale,
        offset: 0.0,
    }
}

const INCH: f64 = 0.0254;
const FOOT: f64 = 0.3048;
const US_GALLON: f64 = 0.003_785_411_784;
const POUND: f64 = 0.453_592_37;
const STANDARD_GRAVITY: f64 = 9.806_65;
const BTU: f64 = 1_055.055_852_62;

/// Every built-in unit.
pub const UNITS: &[Unit] = &[
    unit("", "number", Dimension::None, 1.0),
    unit("%", "percent", Dimension::None, 0.01),
    unit("m", "metre", Dimension::Length, 1.0),
    unit("mm", "millimetre", Dimension::Length, 0.001),
    unit("cm", "centimetre", Dimension::Length, 0.01),
    unit("km", "kilometre", Dimension::Length, 1000.0),
    unit("um", "micrometre", Dimension::Length, 1e-6),
    unit("in", "inch", Dimension::Length, INCH),
    unit("ft", "foot", Dimension::Length, FOOT),
    unit("yd", "yard", Dimension::Length, 0.9144),
    unit("mi", "mile", Dimension::Length, 1_609.344),
    unit("pt", "typographic point", Dimension::Length, INCH / 72.0),
    unit("pc", "pica", Dimension::Length, INCH / 6.0),
    unit("px", "CSS pixel", Dimension::Length, INCH / 96.0),
    unit("m2", "square metre", Dimension::Area, 1.0),
    unit("mm2", "square millimetre", Dimension::Area, 1e-6),
    unit("cm2", "square centimetre", Dimension::Area, 1e-4),
    unit("ft2", "square foot", Dimension::Area, FOOT * FOOT),
    unit("in2", "square inch", Dimension::Area, INCH * INCH),
    unit("m3", "cubic metre", Dimension::Volume, 1.0),
    unit("L", "litre", Dimension::Volume, 0.001),
    unit("mL", "millilitre", Dimension::Volume, 1e-6),
    unit("ft3", "cubic foot", Dimension::Volume, FOOT * FOOT * FOOT),
    unit("gal", "US gallon", Dimension::Volume, US_GALLON),
    unit("rad", "radian", Dimension::Angle, 1.0),
    unit(
        "deg",
        "degree",
        Dimension::Angle,
        std::f64::consts::PI / 180.0,
    ),
    unit(
        "grad",
        "gradian",
        Dimension::Angle,
        std::f64::consts::PI / 200.0,
    ),
    unit("turn", "turn", Dimension::Angle, std::f64::consts::TAU),
    unit("s", "second", Dimension::Time, 1.0),
    unit("ms", "millisecond", Dimension::Time, 0.001),
    unit("us", "microsecond", Dimension::Time, 1e-6),
    unit("min", "minute", Dimension::Time, 60.0),
    unit("h", "hour", Dimension::Time, 3600.0),
    unit("Hz", "hertz", Dimension::Frequency, 1.0),
    unit("kHz", "kilohertz", Dimension::Frequency, 1e3),
    unit("MHz", "megahertz", Dimension::Frequency, 1e6),
    unit(
        "rpm",
        "revolution per minute",
        Dimension::Frequency,
        1.0 / 60.0,
    ),
    unit("kg", "kilogram", Dimension::Mass, 1.0),
    unit("g", "gram", Dimension::Mass, 0.001),
    unit("t", "tonne", Dimension::Mass, 1000.0),
    unit("lb", "pound", Dimension::Mass, POUND),
    unit("oz", "ounce", Dimension::Mass, POUND / 16.0),
    unit("N", "newton", Dimension::Force, 1.0),
    unit("kN", "kilonewton", Dimension::Force, 1e3),
    unit(
        "lbf",
        "pound-force",
        Dimension::Force,
        POUND * STANDARD_GRAVITY,
    ),
    unit("Pa", "pascal", Dimension::Pressure, 1.0),
    unit("kPa", "kilopascal", Dimension::Pressure, 1e3),
    unit("MPa", "megapascal", Dimension::Pressure, 1e6),
    unit("bar", "bar", Dimension::Pressure, 1e5),
    unit("atm", "standard atmosphere", Dimension::Pressure, 101_325.0),
    unit(
        "psi",
        "pound per square inch",
        Dimension::Pressure,
        POUND * STANDARD_GRAVITY / (INCH * INCH),
    ),
    unit(
        "inH2O",
        "inch of water",
        Dimension::Pressure,
        249.088_908_333,
    ),
    unit("J", "joule", Dimension::Energy, 1.0),
    unit("kJ", "kilojoule", Dimension::Energy, 1e3),
    unit("Wh", "watt-hour", Dimension::Energy, 3600.0),
    unit("kWh", "kilowatt-hour", Dimension::Energy, 3.6e6),
    unit("cal", "calorie", Dimension::Energy, 4.184),
    unit("BTU", "British thermal unit", Dimension::Energy, BTU),
    unit("W", "watt", Dimension::Power, 1.0),
    unit("kW", "kilowatt", Dimension::Power, 1e3),
    unit("MW", "megawatt", Dimension::Power, 1e6),
    unit(
        "hp",
        "mechanical horsepower",
        Dimension::Power,
        745.699_871_582,
    ),
    unit("BTU/h", "BTU per hour", Dimension::Power, BTU / 3600.0),
    unit(
        "TR",
        "ton of refrigeration",
        Dimension::Power,
        12_000.0 * BTU / 3600.0,
    ),
    Unit {
        symbol: "K",
        name: "kelvin",
        dimension: Dimension::Temperature,
        scale: 1.0,
        offset: 0.0,
    },
    Unit {
        symbol: "degC",
        name: "degree Celsius",
        dimension: Dimension::Temperature,
        scale: 1.0,
        offset: 273.15,
    },
    Unit {
        symbol: "degF",
        name: "degree Fahrenheit",
        dimension: Dimension::Temperature,
        scale: 5.0 / 9.0,
        offset: 273.15 - 32.0 * 5.0 / 9.0,
    },
    unit("m/s", "metre per second", Dimension::Velocity, 1.0),
    unit("km/h", "kilometre per hour", Dimension::Velocity, 1.0 / 3.6),
    unit(
        "mph",
        "mile per hour",
        Dimension::Velocity,
        1_609.344 / 3600.0,
    ),
    unit("fpm", "foot per minute", Dimension::Velocity, FOOT / 60.0),
    unit("m3/s", "cubic metre per second", Dimension::FlowRate, 1.0),
    unit("L/s", "litre per second", Dimension::FlowRate, 0.001),
    unit(
        "L/min",
        "litre per minute",
        Dimension::FlowRate,
        0.001 / 60.0,
    ),
    unit(
        "cfm",
        "cubic foot per minute",
        Dimension::FlowRate,
        FOOT * FOOT * FOOT / 60.0,
    ),
    unit(
        "gpm",
        "US gallon per minute",
        Dimension::FlowRate,
        US_GALLON / 60.0,
    ),
    unit("V", "volt", Dimension::Voltage, 1.0),
    unit("mV", "millivolt", Dimension::Voltage, 1e-3),
    unit("kV", "kilovolt", Dimension::Voltage, 1e3),
    unit("A", "ampere", Dimension::Current, 1.0),
    unit("mA", "milliampere", Dimension::Current, 1e-3),
    unit("ohm", "ohm", Dimension::Resistance, 1.0),
    unit("kohm", "kilohm", Dimension::Resistance, 1e3),
    unit("Mohm", "megohm", Dimension::Resistance, 1e6),
    unit("F", "farad", Dimension::Capacitance, 1.0),
    unit("uF", "microfarad", Dimension::Capacitance, 1e-6),
    unit("nF", "nanofarad", Dimension::Capacitance, 1e-9),
    unit("pF", "picofarad", Dimension::Capacitance, 1e-12),
    unit("H", "henry", Dimension::Inductance, 1.0),
    unit("mH", "millihenry", Dimension::Inductance, 1e-3),
    unit("cd", "candela", Dimension::LuminousIntensity, 1.0),
    unit("B", "byte", Dimension::Data, 1.0),
    unit("KiB", "kibibyte", Dimension::Data, 1024.0),
    unit("MiB", "mebibyte", Dimension::Data, 1024.0 * 1024.0),
    unit("GiB", "gibibyte", Dimension::Data, 1024.0 * 1024.0 * 1024.0),
    unit("dB", "decibel", Dimension::Level, 1.0),
];

impl Unit {
    /// The unit with `symbol`.
    #[must_use]
    pub fn find(symbol: &str) -> Option<Self> {
        UNITS.iter().copied().find(|unit| unit.symbol == symbol)
    }

    /// The SI unit of `dimension` (the one with scale 1 and no offset).
    #[must_use]
    pub fn base(dimension: Dimension) -> Option<Self> {
        UNITS
            .iter()
            .copied()
            .find(|unit| unit.dimension == dimension && unit.scale == 1.0 && unit.offset == 0.0)
    }
}

/// A number with a unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quantity {
    /// The number, in `unit`.
    pub value: f64,
    /// The unit.
    pub unit: Unit,
}

/// Two quantities of different dimensions met.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DimensionMismatch {
    /// What was there.
    pub found: Dimension,
    /// What was needed.
    pub expected: Dimension,
}

impl fmt::Display for DimensionMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "expected a {:?}, found a {:?}",
            self.expected, self.found
        )
    }
}

impl std::error::Error for DimensionMismatch {}

impl Quantity {
    /// `value` in the unit with `symbol`.
    ///
    /// # Panics
    /// If no unit has `symbol` (a programming error; use `parse` for input).
    #[must_use]
    pub fn new(value: f64, symbol: &str) -> Self {
        Self {
            value,
            unit: Unit::find(symbol).unwrap_or_else(|| panic!("unknown unit `{symbol}`")),
        }
    }

    /// The value in SI base units.
    #[must_use]
    pub fn si(&self) -> f64 {
        self.value * self.unit.scale + self.unit.offset
    }

    /// The same quantity in `target`.
    ///
    /// # Errors
    /// If `target` measures something else.
    pub fn to(&self, target: Unit) -> Result<Self, DimensionMismatch> {
        if target.dimension != self.unit.dimension {
            return Err(DimensionMismatch {
                found: self.unit.dimension,
                expected: target.dimension,
            });
        }
        Ok(Self {
            value: (self.si() - target.offset) / target.scale,
            unit: target,
        })
    }

    /// The value in the unit with `symbol`, if it measures the same thing.
    #[must_use]
    pub fn in_unit(&self, symbol: &str) -> Option<f64> {
        self.to(Unit::find(symbol)?)
            .ok()
            .map(|quantity| quantity.value)
    }

    /// Adds `other`, converting it into this quantity's unit.
    ///
    /// # Errors
    /// If the dimensions differ (or for temperatures, which do not add).
    pub fn add(&self, other: &Self) -> Result<Self, DimensionMismatch> {
        if self.unit.dimension == Dimension::Temperature {
            return Err(DimensionMismatch {
                found: Dimension::Temperature,
                expected: Dimension::None,
            });
        }
        let other = other.to(self.unit)?;
        Ok(Self {
            value: self.value + other.value,
            unit: self.unit,
        })
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.unit.symbol.is_empty() {
            write!(f, "{}", self.value)
        } else {
            write!(f, "{} {}", self.value, self.unit.symbol)
        }
    }
}

impl FromStr for Quantity {
    type Err = String;

    /// Parses `12.5 mm`, `12.5mm` or a bare number.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        let split = text
            .char_indices()
            .find(|(index, ch)| {
                !(ch.is_ascii_digit()
                    || *ch == '.'
                    || ((*ch == '-' || *ch == '+') && *index == 0)
                    || ((*ch == 'e' || *ch == 'E')
                        && text[index + 1..]
                            .starts_with(|next: char| next.is_ascii_digit() || next == '-')))
            })
            .map_or(text.len(), |(index, _)| index);
        let (number, symbol) = text.split_at(split);
        let value: f64 = number
            .trim()
            .parse()
            .map_err(|_| format!("`{text}` does not start with a number"))?;
        let unit =
            Unit::find(symbol.trim()).ok_or_else(|| format!("unknown unit `{}`", symbol.trim()))?;
        Ok(Self { value, unit })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9 * a.abs().max(b.abs()).max(1.0)
    }

    #[test]
    fn conversions_are_exact_within_a_dimension() {
        let length = Quantity::new(1.0, "in");
        assert!(close(length.in_unit("mm").unwrap(), 25.4));
        assert!(close(Quantity::new(72.0, "pt").in_unit("in").unwrap(), 1.0));
        assert!(close(
            Quantity::new(100.0, "degC").in_unit("degF").unwrap(),
            212.0
        ));
        assert!(close(
            Quantity::new(0.0, "degC").in_unit("K").unwrap(),
            273.15
        ));
        assert!(close(
            Quantity::new(1.0, "TR").in_unit("BTU/h").unwrap(),
            12_000.0
        ));
        assert!(close(
            Quantity::new(1.0, "psi").in_unit("kPa").unwrap(),
            6.894_757_293
        ));
        assert!(close(
            Quantity::new(1000.0, "cfm").in_unit("L/s").unwrap(),
            471.947_443_2
        ));
        assert!(Quantity::new(1.0, "m").in_unit("kg").is_none());
    }

    #[test]
    fn dimensions_never_mix() {
        let sum = Quantity::new(1.0, "m")
            .add(&Quantity::new(50.0, "cm"))
            .unwrap();
        assert!(close(sum.value, 1.5));
        assert!(
            Quantity::new(1.0, "m")
                .add(&Quantity::new(1.0, "Pa"))
                .is_err()
        );
        assert!(
            Quantity::new(1.0, "degC")
                .add(&Quantity::new(1.0, "degC"))
                .is_err()
        );
    }

    #[test]
    fn quantities_parse_and_print() {
        let parsed: Quantity = "12.5 mm".parse().unwrap();
        assert_eq!(parsed, Quantity::new(12.5, "mm"));
        assert_eq!(
            "-3kPa".parse::<Quantity>().unwrap(),
            Quantity::new(-3.0, "kPa")
        );
        assert_eq!(
            "1e3 Hz".parse::<Quantity>().unwrap(),
            Quantity::new(1000.0, "Hz")
        );
        assert_eq!(
            "42".parse::<Quantity>().unwrap().unit.dimension,
            Dimension::None
        );
        assert!("12 parsecs".parse::<Quantity>().is_err());
        assert_eq!(Quantity::new(2.0, "kW").to_string(), "2 kW");
        let symbols: std::collections::BTreeSet<&str> =
            UNITS.iter().map(|unit| unit.symbol).collect();
        assert_eq!(symbols.len(), UNITS.len(), "symbols are unique");
    }
}
