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

use crate::structs::Command;

mod help;
mod main;
mod other;
mod owner;
mod premium;
mod settings;

pub fn commands() -> Vec<Command> {
    main::commands()
        .into_iter()
        .chain(other::commands())
        .chain(settings::commands())
        .chain(premium::commands())
        .chain(owner::commands())
        .chain(help::commands())
        .collect()
}
