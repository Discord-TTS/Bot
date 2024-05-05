// Discord TTS Bot
// Copyright (C) 2021-Present David Thomas
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

#[macro_export]
macro_rules! require {
    ($to_check:expr) => {
        require!($to_check, ())
    };
    ($to_check:expr, $ret:expr) => {
        if let Some(to_check) = $to_check {
            to_check
        } else {
            return $ret;
        }
    };
}

#[macro_export]
macro_rules! require_guild {
    ($ctx:expr) => {
        require_guild!($ctx, Ok(()))
    };
    ($ctx:expr, $ret:expr) => {
        $crate::require!($ctx.guild(), {
            ::tracing::warn!(
                "Guild {} not cached in {} command!",
                $ctx.guild_id().unwrap(),
                $ctx.command().qualified_name
            );
            $ret
        })
    };
}

#[macro_export]
macro_rules! bool_enum {
    ($name:ident($true_value:ident | $false_value:ident)) => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        pub enum $name {
            $true_value,
            $false_value,
        }

        impl From<$name> for bool {
            fn from(value: $name) -> bool {
                value == $name::$true_value
            }
        }

        impl From<bool> for $name {
            fn from(value: bool) -> Self {
                if value {
                    Self::$true_value
                } else {
                    Self::$false_value
                }
            }
        }
    };
}
