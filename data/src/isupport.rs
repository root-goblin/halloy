use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use chrono::format::SecondsFormat;
use chrono::{DateTime, Utc};
use irc::proto;

use crate::Message;
use crate::target::Target;

// Utilized ISUPPORT parameters should have an associated Kind enum variant
// returned by Operation::kind() and Parameter::kind()
#[allow(non_camel_case_types)]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Kind {
    AWAYLEN,
    CASEMAPPING,
    CHANLIMIT,
    CHANMODES,
    CHANNELLEN,
    CHANTYPES,
    CHATHISTORY,
    CNOTICE,
    CPRIVMSG,
    ELIST,
    KEYLEN,
    KICKLEN,
    KNOCK,
    MODES,
    MONITOR,
    MSGREFTYPES,
    NAMELEN,
    NICKLEN,
    PREFIX,
    SAFELIST,
    STATUSMSG,
    TARGMAX,
    TOPICLEN,
    USERIP,
    UTF8ONLY,
    WHOX,
}

#[derive(Debug)]
pub enum Operation {
    Add(Parameter),
    Remove(String),
}

impl FromStr for Operation {
    type Err = &'static str;

    fn from_str(token: &str) -> Result<Self, Self::Err> {
        if token.is_empty() {
            return Err("empty ISUPPORT token not allowed");
        }

        match token.chars().next() {
            Some('-') => Ok(Operation::Remove(token.chars().skip(1).collect())),
            _ => {
                if let Some((parameter, value)) = token.split_once('=') {
                    match parameter {
                        "ACCEPT" => Ok(Operation::Add(Parameter::ACCEPT(
                            parse_required_positive_integer(value)?,
                        ))),
                        "ACCOUNTEXTBAN" => {
                            let account_based_extended_ban_masks = value
                                .split(',')
                                .map(String::from)
                                .collect::<Vec<_>>();

                            if !account_based_extended_ban_masks.is_empty() {
                                Ok(Operation::Add(Parameter::ACCOUNTEXTBAN(
                                    account_based_extended_ban_masks,
                                )))
                            } else {
                                Err("no valid account-based extended ban masks")
                            }
                        }
                        "AWAYLEN" => Ok(Operation::Add(Parameter::AWAYLEN(
                            parse_required_positive_integer(value)?,
                        ))),
                        "BOT" => Ok(Operation::Add(Parameter::BOT(
                            parse_required_letter(value, None)?,
                        ))),
                        "CALLERID" => Ok(Operation::Add(Parameter::CALLERID(
                            parse_required_letter(
                                value,
                                Some(DEFAULT_CALLER_ID_LETTER),
                            )?,
                        ))),
                        "CASEMAPPING" => match value.to_lowercase().as_ref() {
                            "ascii" => Ok(Operation::Add(
                                Parameter::CASEMAPPING(CaseMap::ASCII),
                            )),
                            "rfc1459" => Ok(Operation::Add(
                                Parameter::CASEMAPPING(CaseMap::RFC1459),
                            )),
                            "rfc1459-strict" => Ok(Operation::Add(
                                Parameter::CASEMAPPING(CaseMap::RFC1459_STRICT),
                            )),
                            "rfc7613" => Ok(Operation::Add(
                                Parameter::CASEMAPPING(CaseMap::RFC7613),
                            )),
                            _ => Err("unknown casemapping"),
                        },
                        "CHANLIMIT" => {
                            let mut channel_limits = vec![];

                            value.split(',').for_each(|channel_limit| {
                                if let Some((prefix, limit)) =
                                    channel_limit.split_once(':')
                                {
                                    if limit.is_empty() {
                                        for c in prefix.chars() {
                                            // TODO validate after STATUSMSG received
                                            channel_limits.push(ChannelLimit {
                                                prefix: c,
                                                limit: None,
                                            });
                                        }
                                    } else if let Ok(limit) =
                                        limit.parse::<u16>()
                                    {
                                        for c in prefix.chars() {
                                            // TODO validate after STATUSMSG received
                                            channel_limits.push(ChannelLimit {
                                                prefix: c,
                                                limit: Some(limit),
                                            });
                                        }
                                    }
                                }
                            });

                            if !channel_limits.is_empty() {
                                Ok(Operation::Add(Parameter::CHANLIMIT(
                                    channel_limits,
                                )))
                            } else {
                                Err("no valid channel limits")
                            }
                        }
                        "CHANMODES" => {
                            let mut channel_modes = vec![];

                            ('A'..='Z').zip(value.split(',')).for_each(
                                |(kind, modes)| {
                                    if modes
                                        .chars()
                                        .all(|c| c.is_ascii_alphabetic())
                                    {
                                        channel_modes.push(ModeKind {
                                            kind,
                                            modes: Cow::Owned(
                                                modes.to_string(),
                                            ),
                                        });
                                    }
                                },
                            );

                            if !channel_modes.is_empty() {
                                Ok(Operation::Add(Parameter::CHANMODES(
                                    channel_modes,
                                )))
                            } else {
                                Err("no valid channel modes")
                            }
                        }
                        "CHANNELLEN" => {
                            Ok(Operation::Add(Parameter::CHANNELLEN(
                                parse_required_positive_integer(value)?,
                            )))
                        }
                        "CHANTYPES" => {
                            let chars = value.chars().collect::<Vec<_>>();
                            if chars.is_empty() {
                                Ok(Operation::Add(Parameter::CHANTYPES(None)))
                            } else {
                                // TODO validate after STATUSMSG is received
                                Ok(Operation::Add(Parameter::CHANTYPES(Some(
                                    chars,
                                ))))
                            }
                        }
                        "CHATHISTORY" | "draft/CHATHISTORY" => {
                            Ok(Operation::Add(Parameter::CHATHISTORY(
                                parse_required_positive_integer(value)?,
                            )))
                        }
                        "CLIENTTAGDENY" => {
                            let mut client_tag_denials = vec![];

                            value.split(',').for_each(|client_tag_denial| {
                                match client_tag_denial.chars().next() {
                                    Some('*') => {
                                        client_tag_denials
                                            .push(ClientOnlyTags::DenyAll);
                                    }
                                    Some('-') => {
                                        client_tag_denials.push(
                                            ClientOnlyTags::Allowed(
                                                client_tag_denial
                                                    .chars()
                                                    .skip(1)
                                                    .collect(),
                                            ),
                                        );
                                    }
                                    _ => client_tag_denials.push(
                                        ClientOnlyTags::Denied(
                                            client_tag_denial.to_string(),
                                        ),
                                    ),
                                }
                            });

                            if !client_tag_denials.is_empty() {
                                Ok(Operation::Add(Parameter::CLIENTTAGDENY(
                                    client_tag_denials,
                                )))
                            } else {
                                Err("no valid client tag denials")
                            }
                        }
                        "CLIENTVER" => {
                            if let Some((major, minor)) = value.split_once('.')
                                && let (Ok(major), Ok(minor)) =
                                    (major.parse::<u16>(), minor.parse::<u16>())
                                {
                                    return Ok(Operation::Add(
                                        Parameter::CLIENTVER(major, minor),
                                    ));
                                }

                            Err(
                                "value must be a <major>.<minor> version number",
                            )
                        }
                        "CNOTICE" => Ok(Operation::Add(Parameter::CNOTICE)),
                        "CPRIVMSG" => Ok(Operation::Add(Parameter::CPRIVMSG)),
                        "DEAF" => Ok(Operation::Add(Parameter::DEAF(
                            parse_required_letter(
                                value,
                                Some(DEFAULT_DEAF_LETTER),
                            )?,
                        ))),
                        "ELIST" => {
                            if !value.is_empty() {
                                let value = value.to_uppercase();

                                if value.chars().all(|c| "CMNTU".contains(c)) {
                                    Ok(Operation::Add(Parameter::ELIST(
                                        value.to_string(),
                                    )))
                                } else {
                                    Err(
                                        "value required to only contain valid search extensions",
                                    )
                                }
                            } else {
                                Err("value required")
                            }
                        }
                        "ESILENCE" => Ok(Operation::Add(Parameter::ESILENCE(
                            parse_optional_letters(value)?,
                        ))),
                        "ETRACE" => Ok(Operation::Add(Parameter::ETRACE)),
                        "EXCEPTS" => Ok(Operation::Add(Parameter::EXCEPTS(
                            parse_required_letter(
                                value,
                                Some(DEFAULT_BAN_EXCEPTION_CHANNEL_LETTER),
                            )?,
                        ))),
                        "EXTBAN" => {
                            if let Some((prefix, types)) = value.split_once(',')
                            {
                                if types
                                    .chars()
                                    .all(|c| c.is_ascii_alphabetic())
                                {
                                    if prefix.is_empty() {
                                        Ok(Operation::Add(Parameter::EXTBAN(
                                            None,
                                            types.to_string(),
                                        )))
                                    } else if prefix.is_ascii() {
                                        Ok(Operation::Add(Parameter::EXTBAN(
                                            prefix.chars().next(),
                                            types.to_string(),
                                        )))
                                    } else {
                                        Err("invalid extended ban prefix(es)")
                                    }
                                } else {
                                    Err("invalid extended ban type(s)")
                                }
                            } else {
                                Err("no valid extended ban masks")
                            }
                        }
                        "FNC" => Ok(Operation::Add(Parameter::FNC)),
                        "HOSTLEN" => Ok(Operation::Add(Parameter::HOSTLEN(
                            parse_required_positive_integer(value)?,
                        ))),
                        "INVEX" => Ok(Operation::Add(Parameter::INVEX(
                            parse_required_letter(
                                value,
                                Some(DEFAULT_INVITE_EXCEPTION_LETTER),
                            )?,
                        ))),
                        "KEYLEN" => Ok(Operation::Add(Parameter::KEYLEN(
                            parse_required_positive_integer(value)?,
                        ))),
                        "KICKLEN" => Ok(Operation::Add(Parameter::KICKLEN(
                            parse_required_positive_integer(value)?,
                        ))),
                        "KNOCK" => Ok(Operation::Add(Parameter::KNOCK)),
                        "LINELEN" => Ok(Operation::Add(Parameter::LINELEN(
                            parse_required_positive_integer(value)?,
                        ))),
                        "MAP" => Ok(Operation::Add(Parameter::MAP)),
                        "MAXBANS" => Ok(Operation::Add(Parameter::MAXBANS(
                            parse_required_positive_integer(value)?,
                        ))),
                        "MAXCHANNELS" => {
                            Ok(Operation::Add(Parameter::MAXCHANNELS(
                                parse_required_positive_integer(value)?,
                            )))
                        }
                        "MAXLIST" => {
                            let mut modes_limits = vec![];

                            value.split(',').for_each(|modes_limit| {
                                if let Some((modes, limit)) =
                                    modes_limit.split_once(':')
                                    && !modes.is_empty()
                                        && modes
                                            .chars()
                                            .all(|c| c.is_ascii_alphabetic())
                                        && let Ok(limit) = limit.parse::<u16>()
                                        {
                                            modes_limits.push(ModesLimit {
                                                modes: modes.to_string(),
                                                limit,
                                            });
                                        }
                            });

                            if !modes_limits.is_empty() {
                                Ok(Operation::Add(Parameter::MAXLIST(
                                    modes_limits,
                                )))
                            } else {
                                Err("no valid modes limits")
                            }
                        }
                        "MAXPARA" => Ok(Operation::Add(Parameter::MAXPARA(
                            parse_required_positive_integer(value)?,
                        ))),
                        "MAXTARGETS" => {
                            Ok(Operation::Add(Parameter::MAXTARGETS(
                                parse_optional_positive_integer(value)?,
                            )))
                        }
                        "METADATA" => Ok(Operation::Add(Parameter::METADATA(
                            parse_optional_positive_integer(value)?,
                        ))),
                        "MODES" => Ok(Operation::Add(Parameter::MODES(
                            parse_optional_positive_integer(value)?,
                        ))),
                        "MONITOR" => Ok(Operation::Add(Parameter::MONITOR(
                            parse_optional_positive_integer(value)?,
                        ))),
                        "MSGREFTYPES" => {
                            let mut message_reference_types = vec![];

                            value.split(',').for_each(
                                |message_reference_type| {
                                    match message_reference_type {
                                        "msgid" => message_reference_types
                                            .insert(
                                                0,
                                                MessageReferenceType::MessageId,
                                            ),
                                        "timestamp" => message_reference_types
                                            .insert(
                                                0,
                                                MessageReferenceType::Timestamp,
                                            ),
                                        _ => (),
                                    }
                                },
                            );

                            Ok(Operation::Add(Parameter::MSGREFTYPES(
                                message_reference_types,
                            )))
                        }
                        "NAMELEN" => Ok(Operation::Add(Parameter::NAMELEN(
                            parse_required_positive_integer(value)?,
                        ))),
                        "NAMESX" => Ok(Operation::Add(Parameter::NAMESX)),
                        "NETWORK" => Ok(Operation::Add(Parameter::NETWORK(
                            value.to_string(),
                        ))),
                        "NICKLEN" | "MAXNICKLEN" => {
                            Ok(Operation::Add(Parameter::NICKLEN(
                                parse_required_positive_integer(value)?,
                            )))
                        }
                        "OVERRIDE" => Ok(Operation::Add(Parameter::OVERRIDE)),
                        "PREFIX" => {
                            let mut prefix_maps = vec![];

                            if let Some((modes, prefixes)) =
                                value.split_once(')')
                            {
                                for (mode, prefix) in
                                    modes.chars().skip(1).zip(prefixes.chars())
                                {
                                    prefix_maps
                                        .push(PrefixMap { mode, prefix });
                                }

                                Ok(Operation::Add(Parameter::PREFIX(
                                    prefix_maps,
                                )))
                            } else {
                                Err("unrecognized PREFIX format")
                            }
                        }
                        "SAFELIST" => Ok(Operation::Add(Parameter::SAFELIST)),
                        "SECURELIST" => {
                            Ok(Operation::Add(Parameter::SECURELIST))
                        }
                        "SILENCE" => Ok(Operation::Add(Parameter::SILENCE(
                            parse_optional_positive_integer(value)?,
                        ))),
                        "STATUSMSG" => {
                            let chars = value.chars().collect::<Vec<_>>();
                            // TODO validate that STATUSMSG ⊂ PREFIX after ISUPPORT ends
                            Ok(Operation::Add(Parameter::STATUSMSG(chars)))
                        }
                        "TARGMAX" => {
                            let mut command_target_limits = vec![];

                            value.split(',').for_each(|command_target_limit| {
                                if let Some((command, limit)) =
                                    command_target_limit.split_once(':')
                                    && !command.is_empty()
                                        && command
                                            .chars()
                                            .all(|c| c.is_ascii_alphabetic())
                                    {
                                        if limit.is_empty() {
                                            command_target_limits.push(
                                                CommandTargetLimit {
                                                    command: command
                                                        .to_uppercase()
                                                        .to_string(),
                                                    limit: None,
                                                },
                                            );
                                        } else if let Ok(limit) =
                                            limit.parse::<u16>()
                                        {
                                            command_target_limits.push(
                                                CommandTargetLimit {
                                                    command: command
                                                        .to_uppercase()
                                                        .to_string(),
                                                    limit: Some(limit),
                                                },
                                            );
                                        }
                                    }
                            });

                            if !command_target_limits.is_empty() {
                                Ok(Operation::Add(Parameter::TARGMAX(
                                    command_target_limits,
                                )))
                            } else {
                                Err("no valid command target limits")
                            }
                        }
                        "TOPICLEN" => Ok(Operation::Add(Parameter::TOPICLEN(
                            parse_required_positive_integer(value)?,
                        ))),
                        "UHNAMES" => Ok(Operation::Add(Parameter::UHNAMES)),
                        "USERIP" => Ok(Operation::Add(Parameter::USERIP)),
                        "USERLEN" => Ok(Operation::Add(Parameter::USERLEN(
                            parse_required_positive_integer(value)?,
                        ))),
                        "UTF8ONLY" => Ok(Operation::Add(Parameter::UTF8ONLY)),
                        "VLIST" => Ok(Operation::Add(Parameter::VLIST(
                            parse_required_letters(value)?,
                        ))),
                        "WATCH" => Ok(Operation::Add(Parameter::WATCH(
                            parse_required_positive_integer(value)?,
                        ))),
                        "WHOX" => Ok(Operation::Add(Parameter::WHOX)),
                        _ => Err("unknown ISUPPORT parameter"),
                    }
                } else {
                    match token {
                        "ACCEPT" => Err("value required"),
                        "ACCOUNTEXTBAN" => Err("value(s) required"),
                        "AWAYLEN" => Err("value required"),
                        "BOT" => Err("value required"),
                        "CALLERID" => Ok(Operation::Add(Parameter::CALLERID(
                            DEFAULT_CALLER_ID_LETTER,
                        ))),
                        "CASEMAPPING" => Err("value required"),
                        "CHANLIMIT" => Err("value(s) required"),
                        "CHANMODES" => Err("value(s) required"),
                        "CHANNELLEN" => Err("value required"),
                        "CHANTYPES" => {
                            Ok(Operation::Add(Parameter::CHANTYPES(None)))
                        }
                        "CHATHISTORY" => Err("value required"),
                        "CLIENTTAGDENY" => Err("value(s) required"),
                        "CLIENTVER" => Err("value required"),
                        "DEAF" => Ok(Operation::Add(Parameter::DEAF(
                            DEFAULT_DEAF_LETTER,
                        ))),
                        "ELIST" => Err("value required"),
                        "ESILENCE" => {
                            Ok(Operation::Add(Parameter::ESILENCE(None)))
                        }
                        "ETRACE" => Ok(Operation::Add(Parameter::ETRACE)),
                        "EXCEPTS" => Ok(Operation::Add(Parameter::EXCEPTS(
                            DEFAULT_BAN_EXCEPTION_CHANNEL_LETTER,
                        ))),
                        "EXTBAN" => Err("value required"),
                        "FNC" => Ok(Operation::Add(Parameter::FNC)),
                        "HOSTLEN" => Err("value required"),
                        "INVEX" => Ok(Operation::Add(Parameter::INVEX(
                            DEFAULT_INVITE_EXCEPTION_LETTER,
                        ))),
                        "KEYLEN" => Err("value required"),
                        "KICKLEN" => Err("value required"),
                        "KNOCK" => Ok(Operation::Add(Parameter::KNOCK)),
                        "LINELEN" => Err("value required"),
                        "MAP" => Ok(Operation::Add(Parameter::MAP)),
                        "MAXBANS" => Err("value required"),
                        "MAXCHANNELS" => Err("value required"),
                        "MAXLIST" => Err("value(s) required"),
                        "MAXPARA" => Err("value required"),
                        "MAXTARGETS" => {
                            Ok(Operation::Add(Parameter::MAXTARGETS(None)))
                        }
                        "METADATA" => {
                            Ok(Operation::Add(Parameter::METADATA(None)))
                        }
                        "MODES" => Ok(Operation::Add(Parameter::MODES(None))),
                        "MONITOR" => {
                            Ok(Operation::Add(Parameter::MONITOR(None)))
                        }
                        "MSGREFTYPES" => {
                            Ok(Operation::Add(Parameter::MSGREFTYPES(vec![])))
                        }
                        "NAMESX" => Ok(Operation::Add(Parameter::NAMESX)),
                        "NAMELEN" => Err("value required"),
                        "NETWORK" => Err("value required"),
                        "NICKLEN" | "MAXNICKLEN" => Err("value required"),
                        "OVERRIDE" => Ok(Operation::Add(Parameter::OVERRIDE)),
                        "PREFIX" => {
                            Ok(Operation::Add(Parameter::PREFIX(vec![])))
                        }
                        "SAFELIST" => Ok(Operation::Add(Parameter::SAFELIST)),
                        "SECURELIST" => {
                            Ok(Operation::Add(Parameter::SECURELIST))
                        }
                        "SILENCE" => {
                            Ok(Operation::Add(Parameter::SILENCE(None)))
                        }
                        "STATUSMSG" => Err("value required"),
                        "TARGMAX" => {
                            Ok(Operation::Add(Parameter::TARGMAX(vec![])))
                        }
                        "TOPICLEN" => Err("value required"),
                        "UHNAMES" => Ok(Operation::Add(Parameter::UHNAMES)),
                        "USERIP" => Ok(Operation::Add(Parameter::USERIP)),
                        "USERLEN" => Err("value required"),
                        "UTF8ONLY" => Ok(Operation::Add(Parameter::UTF8ONLY)),
                        "VLIST" => Err("value required"),
                        "WATCH" => Err("value required"),
                        "WHOX" => Ok(Operation::Add(Parameter::WHOX)),
                        _ => Err("unknown ISUPPORT parameter"),
                    }
                }
            }
        }
    }
}

impl Operation {
    pub fn kind(&self) -> Option<Kind> {
        match self {
            Operation::Add(parameter) => parameter.kind(),
            Operation::Remove(parameter) => match parameter.as_ref() {
                "AWAYLEN" => Some(Kind::AWAYLEN),
                "CASEMAPPING" => Some(Kind::CASEMAPPING),
                "CHANLIMIT" => Some(Kind::CHANLIMIT),
                "CHANMODES" => Some(Kind::CHANMODES),
                "CHANNELLEN" => Some(Kind::CHANNELLEN),
                "CHANTYPES" => Some(Kind::CHANTYPES),
                "CHATHISTORY" => Some(Kind::CHATHISTORY),
                "CNOTICE" => Some(Kind::CNOTICE),
                "CPRIVMSG" => Some(Kind::CPRIVMSG),
                "ELIST" => Some(Kind::ELIST),
                "KEYLEN" => Some(Kind::KEYLEN),
                "KICKLEN" => Some(Kind::KICKLEN),
                "KNOCK" => Some(Kind::KNOCK),
                "MODES" => Some(Kind::MODES),
                "MONITOR" => Some(Kind::MONITOR),
                "MSGREFTYPES" => Some(Kind::MSGREFTYPES),
                "NAMELEN" => Some(Kind::NAMELEN),
                "NICKLEN" => Some(Kind::NICKLEN),
                "PREFIX" => Some(Kind::PREFIX),
                "SAFELIST" => Some(Kind::SAFELIST),
                "STATUSMSG" => Some(Kind::STATUSMSG),
                "TARGMAX" => Some(Kind::TARGMAX),
                "TOPICLEN" => Some(Kind::TOPICLEN),
                "USERIP" => Some(Kind::USERIP),
                "UTF8ONLY" => Some(Kind::UTF8ONLY),
                "WHOX" => Some(Kind::WHOX),
                _ => None,
            },
        }
    }
}

// ISUPPORT Parameter References
// - https://defs.ircdocs.horse/defs/isupport.html
// - https://modern.ircdocs.horse/#rplisupport-005
// - https://ircv3.net/specs/extensions/chathistory
// - https://ircv3.net/specs/extensions/monitor
// - https://ircv3.net/specs/extensions/utf8-only
// - https://ircv3.net/specs/extensions/whox
// - https://github.com/ircv3/ircv3-specifications/pull/464/files
#[allow(non_camel_case_types)]
#[derive(Clone, Debug)]
pub enum Parameter {
    ACCEPT(u16),
    ACCOUNTEXTBAN(Vec<String>),
    AWAYLEN(u16),
    BOT(char),
    CALLERID(char),
    CASEMAPPING(CaseMap),
    CHANLIMIT(Vec<ChannelLimit>),
    CHANMODES(Vec<ModeKind>),
    CHANNELLEN(u16),
    CHANTYPES(Option<Vec<char>>),
    CHATHISTORY(u16),
    CLIENTTAGDENY(Vec<ClientOnlyTags>),
    CLIENTVER(u16, u16),
    CNOTICE,
    CPRIVMSG,
    DEAF(char),
    ELIST(String),
    ESILENCE(Option<String>),
    ETRACE,
    EXCEPTS(char),
    EXTBAN(Option<char>, String),
    FNC,
    HOSTLEN(u16),
    INVEX(char),
    KEYLEN(u16),
    KICKLEN(u16),
    KNOCK,
    LINELEN(u16),
    MAP,
    MAXBANS(u16),
    MAXCHANNELS(u16),
    MAXLIST(Vec<ModesLimit>),
    MAXPARA(u16),
    MAXTARGETS(Option<u16>),
    METADATA(Option<u16>),
    MODES(Option<u16>),
    MONITOR(Option<u16>),
    MSGREFTYPES(Vec<MessageReferenceType>),
    NAMELEN(u16),
    NAMESX,
    NETWORK(String),
    NICKLEN(u16),
    OVERRIDE,
    PREFIX(Vec<PrefixMap>),
    SAFELIST,
    SECURELIST,
    SILENCE(Option<u16>),
    STATUSMSG(Vec<char>),
    TARGMAX(Vec<CommandTargetLimit>),
    TOPICLEN(u16),
    UHNAMES,
    USERIP,
    USERLEN(u16),
    UTF8ONLY,
    VLIST(String),
    WATCH(u16),
    WHOX,
}

impl Parameter {
    pub fn kind(&self) -> Option<Kind> {
        match self {
            Parameter::AWAYLEN(_) => Some(Kind::AWAYLEN),
            Parameter::CASEMAPPING(_) => Some(Kind::CASEMAPPING),
            Parameter::CHANLIMIT(_) => Some(Kind::CHANLIMIT),
            Parameter::CHANMODES(_) => Some(Kind::CHANMODES),
            Parameter::CHANNELLEN(_) => Some(Kind::CHANNELLEN),
            Parameter::CHANTYPES(_) => Some(Kind::CHANTYPES),
            Parameter::CHATHISTORY(_) => Some(Kind::CHATHISTORY),
            Parameter::CNOTICE => Some(Kind::CNOTICE),
            Parameter::CPRIVMSG => Some(Kind::CPRIVMSG),
            Parameter::ELIST(_) => Some(Kind::ELIST),
            Parameter::KEYLEN(_) => Some(Kind::KEYLEN),
            Parameter::KICKLEN(_) => Some(Kind::KICKLEN),
            Parameter::KNOCK => Some(Kind::KNOCK),
            Parameter::MODES(_) => Some(Kind::MODES),
            Parameter::MONITOR(_) => Some(Kind::MONITOR),
            Parameter::MSGREFTYPES(_) => Some(Kind::MSGREFTYPES),
            Parameter::NAMELEN(_) => Some(Kind::NAMELEN),
            Parameter::NICKLEN(_) => Some(Kind::NICKLEN),
            Parameter::PREFIX(_) => Some(Kind::PREFIX),
            Parameter::SAFELIST => Some(Kind::SAFELIST),
            Parameter::STATUSMSG(_) => Some(Kind::STATUSMSG),
            Parameter::TARGMAX(_) => Some(Kind::TARGMAX),
            Parameter::TOPICLEN(_) => Some(Kind::TOPICLEN),
            Parameter::USERIP => Some(Kind::USERIP),
            Parameter::UTF8ONLY => Some(Kind::UTF8ONLY),
            Parameter::WHOX => Some(Kind::WHOX),
            _ => None,
        }
    }
}

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Default)]
pub enum CaseMap {
    ASCII,
    RFC1459,
    RFC1459_STRICT,
    #[default]
    RFC7613,
}

impl CaseMap {
    pub fn normalize(&self, from_str: &str) -> String {
        match self {
            CaseMap::ASCII => from_str.to_ascii_lowercase(),
            CaseMap::RFC1459 => from_str
                .chars()
                .map(|c| match c {
                    'A' => 'a',
                    'B' => 'b',
                    'C' => 'c',
                    'D' => 'd',
                    'E' => 'e',
                    'F' => 'f',
                    'G' => 'g',
                    'H' => 'h',
                    'I' => 'i',
                    'J' => 'j',
                    'K' => 'k',
                    'L' => 'l',
                    'M' => 'm',
                    'N' => 'n',
                    'O' => 'o',
                    'P' => 'p',
                    'Q' => 'q',
                    'R' => 'r',
                    'S' => 's',
                    'T' => 't',
                    'U' => 'u',
                    'V' => 'v',
                    'W' => 'w',
                    'X' => 'x',
                    'Y' => 'y',
                    'Z' => 'z',
                    '[' => '{',
                    ']' => '}',
                    '\\' => '|',
                    '~' => '^',
                    _ => c,
                })
                .collect(),
            CaseMap::RFC1459_STRICT => from_str
                .chars()
                .map(|c| match c {
                    'A' => 'a',
                    'B' => 'b',
                    'C' => 'c',
                    'D' => 'd',
                    'E' => 'e',
                    'F' => 'f',
                    'G' => 'g',
                    'H' => 'h',
                    'I' => 'i',
                    'J' => 'j',
                    'K' => 'k',
                    'L' => 'l',
                    'M' => 'm',
                    'N' => 'n',
                    'O' => 'o',
                    'P' => 'p',
                    'Q' => 'q',
                    'R' => 'r',
                    'S' => 's',
                    'T' => 't',
                    'U' => 'u',
                    'V' => 'v',
                    'W' => 'w',
                    'X' => 'x',
                    'Y' => 'y',
                    'Z' => 'z',
                    '[' => '{',
                    ']' => '}',
                    '\\' => '|',
                    _ => c,
                })
                .collect(),
            CaseMap::RFC7613 => from_str.to_lowercase(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChannelLimit {
    pub prefix: char,
    pub limit: Option<u16>,
}

// Reference: https://datatracker.ietf.org/doc/html/draft-hardy-irc-isupport-00#section-4.3
#[derive(Clone, Debug)]
pub struct ModeKind {
    pub kind: char,
    pub modes: Cow<'static, str>,
}

impl fmt::Display for ModeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            'A' => {
                write!(f, "requires argument to modify & no argument to query")
            }
            'B' => write!(f, "requires argument"),
            'C' => write!(f, "requires argument to set & no argument to clear"),
            'D' => write!(f, "requires no argument"),
            _ => write!(f, "unknown mode type"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChatHistorySubcommand {
    Latest(Target, MessageReference, u16),
    Before(Target, MessageReference, u16),
    Between(Target, MessageReference, MessageReference, u16),
    Targets(MessageReference, MessageReference, u16),
}

impl ChatHistorySubcommand {
    pub fn target(&self) -> Option<&str> {
        match self {
            ChatHistorySubcommand::Latest(target, _, _)
            | ChatHistorySubcommand::Before(target, _, _)
            | ChatHistorySubcommand::Between(target, _, _, _) => {
                Some(target.as_str())
            }
            ChatHistorySubcommand::Targets(_, _, _) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChatHistoryState {
    Exhausted,
    PendingRequest,
    Ready,
}

#[derive(Clone, Debug)]
pub enum ClientOnlyTags {
    Allowed(String),
    Denied(String),
    DenyAll,
}

#[derive(Clone, Debug)]
pub struct CommandTargetLimit {
    pub command: String,
    pub limit: Option<u16>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MessageReference {
    Timestamp(DateTime<Utc>),
    MessageId(String),
    None,
}

impl fmt::Display for MessageReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageReference::Timestamp(server_time) => write!(
                f,
                "timestamp={}",
                server_time.to_rfc3339_opts(SecondsFormat::Millis, true)
            ),
            MessageReference::MessageId(id) => write!(f, "msgid={id}"),
            MessageReference::None => write!(f, "*"),
        }
    }
}

impl PartialEq<Message> for MessageReference {
    fn eq(&self, other: &Message) -> bool {
        match self {
            MessageReference::Timestamp(server_time) => {
                other.server_time == *server_time
            }
            MessageReference::MessageId(id) => {
                other.id.as_deref() == Some(id.as_str())
            }
            MessageReference::None => false,
        }
    }
}

#[derive(Clone, Debug)]
pub enum MessageReferenceType {
    Timestamp,
    MessageId,
}

#[derive(Clone, Debug)]
pub struct ModesLimit {
    pub modes: String,
    pub limit: u16,
}

#[derive(Clone, Debug)]
pub struct PrefixMap {
    pub prefix: char,
    pub mode: char,
}

const DEFAULT_BAN_EXCEPTION_CHANNEL_LETTER: char = 'e';

const DEFAULT_CALLER_ID_LETTER: char = 'g';

// Reference: https://modern.ircdocs.horse/#channel-modes
pub const DEFAULT_CHANMODES: &[ModeKind] = &[
    ModeKind {
        kind: 'A',
        modes: Cow::Borrowed("beI"),
    },
    ModeKind {
        kind: 'B',
        modes: Cow::Borrowed("k"),
    },
    ModeKind {
        kind: 'C',
        modes: Cow::Borrowed("l"),
    },
    ModeKind {
        kind: 'D',
        modes: Cow::Borrowed("imstn"),
    },
];

const DEFAULT_DEAF_LETTER: char = 'D';

const DEFAULT_INVITE_EXCEPTION_LETTER: char = 'I';

// Reference: https://modern.ircdocs.horse/#channel-membership-prefixes
const DEFAULT_PREFIX: &[PrefixMap] = &[
    PrefixMap {
        prefix: proto::FOUNDER_PREFIX,
        mode: 'q',
    },
    PrefixMap {
        prefix: proto::PROTECTED_PREFIX_STD,
        mode: 'a',
    },
    PrefixMap {
        prefix: proto::OPERATOR_PREFIX,
        mode: 'o',
    },
    PrefixMap {
        prefix: proto::HALF_OPERATOR_PREFIX,
        mode: 'h',
    },
    PrefixMap {
        prefix: proto::VOICED_PREFIX,
        mode: 'v',
    },
];

const FUZZ_SECONDS: chrono::Duration = chrono::Duration::seconds(5);

pub fn fuzz_start_message_reference(
    message_reference: MessageReference,
) -> MessageReference {
    match message_reference {
        MessageReference::Timestamp(start_server_time) => {
            MessageReference::Timestamp(start_server_time - FUZZ_SECONDS)
        }
        _ => message_reference,
    }
}

pub fn fuzz_end_message_reference(
    message_reference: MessageReference,
) -> MessageReference {
    match message_reference {
        MessageReference::Timestamp(end_server_time) => {
            MessageReference::Timestamp(end_server_time + FUZZ_SECONDS)
        }
        _ => message_reference,
    }
}

pub fn fuzz_message_reference_range(
    first_message_reference: MessageReference,
    second_message_reference: MessageReference,
) -> (MessageReference, MessageReference) {
    match (
        first_message_reference.clone(),
        second_message_reference.clone(),
    ) {
        (
            MessageReference::Timestamp(start_server_time),
            MessageReference::Timestamp(end_server_time),
        ) => {
            if start_server_time < end_server_time {
                (
                    fuzz_start_message_reference(first_message_reference),
                    fuzz_end_message_reference(second_message_reference),
                )
            } else {
                (
                    fuzz_end_message_reference(first_message_reference),
                    fuzz_start_message_reference(second_message_reference),
                )
            }
        }
        _ => (first_message_reference, second_message_reference),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WhoToken {
    digits: [char; 3],
}

impl WhoToken {
    pub fn to_owned(self) -> String {
        self.digits.iter().filter(|c| **c != '\0').collect()
    }
}

impl FromStr for WhoToken {
    type Err = &'static str;

    fn from_str(token: &str) -> Result<Self, Self::Err> {
        if (1usize..=3usize).contains(&token.chars().count())
            && token.chars().all(|c| c.is_ascii_digit())
        {
            let mut digits = ['\0', '\0', '\0'];

            token.chars().enumerate().for_each(|(i, c)| digits[i] = c);

            Ok(WhoToken { digits })
        } else {
            Err("WHO token must be 1-3 ASCII digits")
        }
    }
}

pub enum WhoXPollParameters {
    Default,
    WithAccountName,
}

impl WhoXPollParameters {
    pub fn fields(&self) -> &'static str {
        match self {
            WhoXPollParameters::Default => "tcnf",
            WhoXPollParameters::WithAccountName => "tcnfa",
        }
    }

    pub fn token(&self) -> WhoToken {
        match self {
            WhoXPollParameters::Default => WhoToken {
                digits: ['9', '\0', '\0'],
            },
            WhoXPollParameters::WithAccountName => WhoToken {
                digits: ['9', '9', '\0'],
            },
        }
    }
}

fn parse_optional_letters(value: &str) -> Result<Option<String>, &'static str> {
    if value.is_empty() {
        Ok(None)
    } else if value.chars().all(|c| c.is_ascii_alphabetic()) {
        Ok(Some(value.to_string()))
    } else {
        Err("value required to be letter(s) if specified")
    }
}

fn parse_optional_positive_integer(
    value: &str,
) -> Result<Option<u16>, &'static str> {
    if value.is_empty() {
        Ok(None)
    } else if let Ok(value) = value.parse::<u16>() {
        Ok(Some(value))
    } else {
        Err("optional value must be a positive integer if specified")
    }
}

fn parse_required_letter(
    value: &str,
    default_value: Option<char>,
) -> Result<char, &'static str> {
    if let Some(value) = value.chars().next() {
        if value.is_ascii_alphabetic() {
            return Ok(value);
        }
    } else if let Some(default_value) = default_value {
        return Ok(default_value);
    }

    Err("value required to be a letter")
}

fn parse_required_letters(value: &str) -> Result<String, &'static str> {
    if !value.is_empty() {
        if value.chars().all(|c| c.is_ascii_alphabetic()) {
            Ok(value.to_string())
        } else {
            Err("value required to be letter(s)")
        }
    } else {
        Err("value required")
    }
}

fn parse_required_positive_integer(value: &str) -> Result<u16, &'static str> {
    if let Ok(value) = value.parse::<u16>() {
        Ok(value)
    } else {
        Err("value required to be a positive integer")
    }
}

// Returns the limit directly if found, since we currently treat "no target limit specified"
// the same as "specifying no limit to the number of targets".
pub fn find_target_limit(
    isupport: &HashMap<Kind, Parameter>,
    command: &str,
) -> Option<u16> {
    if let Some(Parameter::TARGMAX(target_limits)) =
        isupport.get(&Kind::TARGMAX)
    {
        target_limits
            .iter()
            .find_map(|target_limit| {
                (target_limit.command == command).then_some(target_limit.limit)
            })
            .flatten()
    } else {
        None
    }
}

pub fn get_casemapping_or_default(
    isupport: &HashMap<Kind, Parameter>,
) -> CaseMap {
    if let Some(Parameter::CASEMAPPING(casemapping)) =
        isupport.get(&Kind::CASEMAPPING)
    {
        return *casemapping;
    }

    CaseMap::default()
}

// https://modern.ircdocs.horse/#chanmodes-parameter
pub fn get_chanmodes_or_default(
    isupport: &HashMap<Kind, Parameter>,
) -> &[ModeKind] {
    isupport
        .get(&Kind::CHANMODES)
        .and_then(|chanmodes| {
            if let Parameter::CHANMODES(modes) = chanmodes {
                Some(modes.as_ref())
            } else {
                log::debug!("Corruption in isupport table.");

                None
            }
        })
        .unwrap_or(DEFAULT_CHANMODES)
}

pub fn get_chantypes_or_default(
    isupport: &HashMap<Kind, Parameter>,
) -> &[char] {
    isupport
        .get(&Kind::CHANTYPES)
        .and_then(|chantypes| {
            if let Parameter::CHANTYPES(types) = chantypes {
                types.as_deref()
            } else {
                log::debug!("Corruption in isupport table.");

                None
            }
        })
        .unwrap_or(proto::DEFAULT_CHANNEL_PREFIXES)
}

// https://modern.ircdocs.horse/#modes-parameter
// The value itself is optional, with None signifying unlimited
pub fn get_mode_limit_or_default(
    isupport: &HashMap<Kind, Parameter>,
) -> Option<u16> {
    isupport
        .get(&Kind::MODES)
        .and_then(|modes| {
            if let Parameter::MODES(mode_limit) = modes {
                Some(*mode_limit)
            } else {
                log::debug!("Corruption in isupport table.");

                None
            }
        })
        .unwrap_or(Some(3))
}

pub fn get_prefix(isupport: &HashMap<Kind, Parameter>) -> Option<&[PrefixMap]> {
    isupport.get(&Kind::PREFIX).and_then(|prefix| {
        if let Parameter::PREFIX(prefix) = prefix {
            Some(prefix.as_ref())
        } else {
            log::debug!("Corruption in isupport table.");

            None
        }
    })
}

pub fn get_prefix_or_default(
    isupport: &HashMap<Kind, Parameter>,
) -> &[PrefixMap] {
    get_prefix(isupport).unwrap_or(DEFAULT_PREFIX)
}

pub fn get_statusmsg_or_default(
    isupport: &HashMap<Kind, Parameter>,
) -> &[char] {
    isupport.get(&Kind::STATUSMSG).map_or(&[], |statusmsg| {
        if let Parameter::STATUSMSG(prefixes) = statusmsg {
            prefixes.as_ref()
        } else {
            log::debug!("Corruption in isupport table.");

            &[]
        }
    })
}
