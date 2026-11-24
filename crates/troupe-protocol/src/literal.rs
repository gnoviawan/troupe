//! Literal wire values (`ok: true`, `type: "gap"`) as zero-sized types, so a
//! frame that carries a constant tag cannot be built with the wrong one and
//! the schema says `const`.

use serde::{Deserialize, Deserializer};

/// Declares a zero-sized type that serializes to one fixed JSON string or
/// boolean and refuses every other value.
macro_rules! literal {
    ($(#[$meta:meta])* $name:ident = str $value:literal) => {
        literal!(@impl $(#[$meta])* $name, $value, serialize_str, String, "string");
    };
    ($(#[$meta:meta])* $name:ident = bool $value:literal) => {
        literal!(@impl $(#[$meta])* $name, $value, serialize_bool, bool, "boolean");
    };
    (@impl $(#[$meta:meta])* $name:ident, $value:literal, $ser:ident, $owned:ty,
        $json_type:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
        pub struct $name;

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.$ser($value)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let found = <$owned as ::serde::Deserialize>::deserialize(deserializer)?;
                if found == $value {
                    Ok(Self)
                } else {
                    Err(<D::Error as ::serde::de::Error>::invalid_value(
                        ::serde::de::Unexpected::Other(&format!("{found:?}")),
                        &stringify!($value),
                    ))
                }
            }
        }

        impl ::schemars::JsonSchema for $name {
            fn inline_schema() -> bool {
                true
            }

            fn schema_name() -> ::std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_: &mut ::schemars::SchemaGenerator) -> ::schemars::Schema {
                ::schemars::json_schema!({ "type": $json_type, "const": $value })
            }
        }
    };
}

literal!(
    /// The JSON value `true`.
    True = bool true
);
literal!(
    /// The JSON value `false`.
    False = bool false
);

/// `deserialize_with` for an optional field whose `null` differs from absent:
/// a present value, `null` included, is `Some`. Pair it with
/// `#[serde(default, skip_serializing_if = "Option::is_none")]`.
pub(crate) fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// Declares a closed set of wire strings as a fieldless enum with its full
/// list (`ALL`) and its wire string (`as_str`).
macro_rules! wire_enum {
    ($(#[$meta:meta])* pub enum $name:ident { $($variant:ident = $wire:literal,)* }) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord,
            ::serde::Serialize, ::serde::Deserialize, ::schemars::JsonSchema,
        )]
        pub enum $name {
            $(#[serde(rename = $wire)] $variant,)*
        }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant,)*];

            /// The wire string.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $wire,)*
                }
            }
        }
    };
}
