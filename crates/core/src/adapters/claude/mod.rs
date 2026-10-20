//! Claude adapter. Claude's data comes from several places, one file each:
//! - `statusline`: the 5-hour and weekly limit %, written by our statusLine script
//! - later: Claude Code transcripts (tokens) and Claude Desktop's usage history (fallback)

pub mod statusline;
