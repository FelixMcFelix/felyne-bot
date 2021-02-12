use serde::{Deserialize, Serialize};

/// Parsed form of the extensions(s) of an RTP packet.
///
/// See [RFC 8285] for the definitions of [`OneByte`]/[`TwoByte`].
///
/// [`OneByte`]: Extension::OneByte
/// [`TwoByte`]: Extension::TwoByte
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[non_exhaustive]
pub enum Extension {
	/// Single-extension variant, containing header data and an optional body.
	Standard(TopExtension, Vec<u8>),
	/// Sub extensions using one-byte headers.
	///
	/// This is parsed where [`TopExtension::info`] `== 0xBEDE`.
	///
	/// [`TopExtension::info`]: TopExtension::info
	OneByte(TopExtension, Vec<SubExtension>),
	/// Sub extensions using two-byte headers.
	///
	/// This is parsed where [`TopExtension::info`] `>> 4 == 0x100`.
	///
	/// [`TopExtension::info`]: TopExtension::info
	TwoByte(TopExtension, Vec<SubExtension>),
}

/// RTP Extension header as observed in all compatible packets.
///
/// The id and length here are as reported, and may not be valid.
/// Packet bodies are discarded if there is currently
/// no logic to properly anonymise them.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct TopExtension {
	/// Info (implementation-specific) identifier field of this header.
	pub info: u16,
	/// Reported length of the top-level extension, in bytes.
	pub length: usize,
}

/// RTP Extension as observed in [`OneByte`] or [`TwoByte`].
///
/// In [`OneByte`], `id` and `length` are reduced to 4 bits each.
///
/// The id and length here are as reported, and may not be valid.
/// Packet bodies are discarded if there is currently
/// no logic to properly anonymise them.
///
/// [`OneByte`]: Extension::OneByte
/// [`TwoByte`]: Extension::TwoByte
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct SubExtension {
	/// Extension type.
	pub id: u8,
	/// Reported length of this extension, in bytes.
	///
	/// For [`OneByte`], this has already been adjusted from [0, 15] -> [1, 16].
	///
	/// [`OneByte`]: Extension::OneByte
	pub length: u8,
	/// Extension body, if available.
	pub body: Vec<u8>,
}

/// A set of top-level extension IDs known to include no user-identifying data.
pub static SAFE_TOP_EXTENSIONS: phf::Set<u16> = phf::phf_set! {
	0xBEDEu16,
};

/// A set of nested extension IDs known to include no user-identifying data.
pub static SAFE_SUB_EXTENSIONS: phf::Set<u8> = phf::phf_set! {
	1u8,
	9u8,
};
