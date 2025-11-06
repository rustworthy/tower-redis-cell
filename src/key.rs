use redis::ToRedisArgs;
use std::fmt::Display;

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Key<'a> {
    String(String),
    Str(&'a str),
    Usize(usize),
    Isize(isize),

    #[cfg(feature = "uuid")]
    #[cfg_attr(docsrs, doc(cfg(feature = "uuid")))]
    Uuid(uuid::Uuid),
}

impl<'a> From<&'a str> for Key<'a> {
    fn from(value: &'a str) -> Self {
        Self::Str(value)
    }
}

impl From<String> for Key<'_> {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl Display for Key<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(value) => value.fmt(f),
            Self::Str(value) => (*value).fmt(f),
            Self::Usize(value) => value.fmt(f),
            Self::Isize(value) => value.fmt(f),
            #[cfg(feature = "uuid")]
            Self::Uuid(value) => value.fmt(f),
        }
    }
}

impl ToRedisArgs for Key<'_> {
    fn write_redis_args<W>(&self, out: &mut W)
    where
        W: ?Sized + redis::RedisWrite,
    {
        match self {
            Self::String(value) => value.write_redis_args(out),
            Self::Str(value) => (*value).write_redis_args(out),
            Self::Usize(value) => value.write_redis_args(out),
            Self::Isize(value) => value.write_redis_args(out),
            #[cfg(feature = "uuid")]
            Self::Uuid(value) => value.write_redis_args(out),
        }
    }
}
