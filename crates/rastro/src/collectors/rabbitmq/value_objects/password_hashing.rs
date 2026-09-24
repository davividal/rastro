//! Which scheme produced a user's stored verifier.

/// The hashing scheme a user's password was stored under.
///
/// **Every scheme RabbitMQ ships salts the password, so the salt is not what tells them
/// apart.** The documented algorithm is the same for all three: generate a random 32-bit
/// salt, prepend it to the password, hash, prepend the salt again, base64 the result. What
/// differs is the cost of testing one candidate password against what the document carries.
///
/// A withheld verifier renders as a digest of itself, and to test a guess against that digest
/// an attacker must guess the 4-byte salt as well: 2^32 hashes per candidate. Under SHA-256
/// and SHA-512 that is a real cost per candidate. Under MD5 it is seconds of ordinary GPU
/// time, so the stand-in for an md5 verifier is a fast offline oracle over any guessable
/// password, and no amount of further hashing by rastro fixes it, because everything needed
/// to recompute it is published beside it.
///
/// **So the verifier is carried only under the two schemes that have been read.** Anything
/// else, md5 included and a scheme a later RabbitMQ adds included, is withheld until somebody
/// has checked how it works. Fail closed, by naming what is allowed rather than what is not,
/// which is the rule the PostgreSQL role verifier already follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordHashing {
    Sha256,
    Sha512,
    Md5,

    /// A scheme rastro has not read, carrying RabbitMQ's own spelling of it.
    Unread(String),
}

/// RabbitMQ's spellings, which are Erlang module names.
const SHA256_MODULE: &str = "rabbit_password_hashing_sha256";
const SHA512_MODULE: &str = "rabbit_password_hashing_sha512";
const MD5_MODULE: &str = "rabbit_password_hashing_md5";

impl PasswordHashing {
    /// The scheme a definitions export named.
    pub fn parse(module: &str) -> Self {
        match module {
            SHA256_MODULE => Self::Sha256,
            SHA512_MODULE => Self::Sha512,
            MD5_MODULE => Self::Md5,
            other => Self::Unread(other.to_owned()),
        }
    }

    /// Whether a verifier stored under this scheme may be carried into the document.
    ///
    /// The one place the rule lives, so a caller cannot get it subtly wrong: adding a variant
    /// makes the compiler ask this question at the only site that answers it.
    pub fn carries_verifier(&self) -> bool {
        match self {
            Self::Sha256 | Self::Sha512 => true,
            Self::Md5 | Self::Unread(_) => false,
        }
    }

    /// rastro's own name for the scheme, which drops the Erlang module prefix.
    ///
    /// An unread scheme keeps its full spelling, because rastro has no name for something it
    /// has not read and shortening it would invent one.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
            Self::Md5 => "md5",
            Self::Unread(module) => module.as_str(),
        }
    }
}
