/*
 * A networking library for the multiplayer game, Conwayste.
 *
 * Copyright (C) 2018-2019 The Conwayste Developers
 *
 * This program is free software: you can redistribute it and/or modify it
 * under the terms of the GNU General Public License as published by the Free
 * Software Foundation, either version 3 of the License, or (at your option)
 * any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of  MERCHANTABILITY or
 * FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for
 * more details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program.  If not, see <http://www.gnu.org/licenses/>.
 */

use std::env;
use std::io::{self, Read, Write};
use std::iter;
use std::str::FromStr;
use std::error::Error;
use std::net::SocketAddr;
use std::process::exit;
use std::time::Instant;
use std::thread;
use std::time::Duration;

use chrono::Local;
use futures::{Future, Sink, Stream, stream, future::ok, sync::mpsc};
use log::LevelFilter;
use regex::Regex;
use tokio_core::reactor::{Core, Timeout};

use crate::net::{
    RequestAction, ResponseCode, Packet,
    BroadcastChatMessage, NetworkManager, NetworkQueue,
    VERSION, has_connection_timed_out, bind, DEFAULT_PORT,
    LineCodec
};

const TICK_INTERVAL_IN_MS:          u64    = 1000;
const NETWORK_INTERVAL_IN_MS:       u64    = 1000;

pub const CLIENT_VERSION: &str = "0.0.1";

//////////////// Event Handling /////////////////
#[derive(PartialEq, Debug, Clone)]
pub enum UserInput {
    Command{cmd: String, args: Vec<String>},
    Chat(String),
}

enum Event {
    TickEvent,
    UserInputEvent(UserInput),
    Incoming((SocketAddr, Option<Packet>)),
    NetworkEvent,
    ConwaysteEvent(RequestAction)
//    NotifyAck((SocketAddr, Option<Packet>)),
}

pub struct ClientNetState {
    pub sequence:         u64,          // Sequence number of requests
    pub response_sequence: u64,         // Value of the next expected sequence number from the server,
                                        // and indicates the sequence number of the next process-able rx packet
    pub name:             Option<String>,
    pub room:             Option<String>,
    pub cookie:           Option<String>,
    pub chat_msg_seq_num: u64,
    pub tick:             usize,
    pub network:          NetworkManager,
    pub heartbeat:        Option<Instant>,
    pub disconnect_initiated: bool,
    pub server_address:   Option<SocketAddr>,
    pub conwayste_tx:     std::sync::mpsc::Sender<ResponseCode>,
}

impl ClientNetState {

    pub fn new(channel_to_conwayste: std::sync::mpsc::Sender<ResponseCode>) -> Self {
        ClientNetState {
            sequence:        0,
            response_sequence: 0,
            name:            None,
            room:            None,
            cookie:          None,
            chat_msg_seq_num: 0,
            tick:            0,
            network:         NetworkManager::new().with_message_buffering(),
            heartbeat:       None,
            disconnect_initiated: false,
            server_address:  None,
            conwayste_tx:    channel_to_conwayste,
        }
    }

    pub fn reset(&mut self) {
        // Design pattern taken from https://blog.getseq.net/rust-at-datalust-how-we-organize-a-complex-rust-codebase/
        // The intention is that new fields added to ClientNetState will cause compiler errors unless
        // we add them here.
        #![deny(unused_variables)]
        let Self {
            ref mut sequence,
            ref mut response_sequence,
            name: ref _name,
            ref mut room,
            ref mut cookie,
            ref mut chat_msg_seq_num,
            ref mut tick,
            ref mut network,
            ref mut heartbeat,
            ref mut disconnect_initiated,
            ref mut server_address,
            conwayste_tx: ref _conwayste_tx,          // Don't clear the channel to conwayste
        } = *self;
        *sequence         = 0;
        *response_sequence = 0;
        *room             = None;
        *cookie           = None;
        *chat_msg_seq_num = 0;
        *tick             = 0;
        *heartbeat        = None;
        *disconnect_initiated = false;
        *server_address   = None;
        network.reset();

        trace!("ClientNetState reset!");
    }

    fn print_help() {
        println!("");
        println!("/help                  - print this text");
        println!("/connect <player_name> - connect to server");
        println!("/disconnect            - disconnect from server");
        println!("/list                  - list rooms when in lobby, or players when in game");
        println!("/new <room_name>       - create a new room (when not in game)");
        println!("/join <room_name>      - join a room (when not in game)");
        println!("/leave                 - leave a room (when in game)");
        println!("/quit                  - exit the program");
        println!("...or just type text to chat!");
    }

    pub fn in_game(&self) -> bool {
        self.room.is_some()
    }

    // XXX Once netwayste integration is complete, we'll need to internally send
    // the result of most of these handlers so we can notify a player via UI event.

    fn check_for_upgrade(&self, server_version: &String) {
        let client_version = &VERSION.to_owned();
        if client_version < server_version {
            warn!("\tClient Version: {}\n\tServer Version: {}\nnWarning: Client out-of-date. Please upgrade.", client_version, server_version);
        }
        else if client_version > server_version {
            warn!("\tClient Version: {}\n\tServer Version: {}\nWarning: Client Version greater than Server Version.", client_version, server_version);
        }
    }

    fn process_queued_server_responses(&mut self) {
        // If we can, start popping off the RX queue and handle contiguous packets immediately
        let mut dequeue_count = 0;

        let rx_queue_count = self.network.rx_packets.get_contiguous_packets_count(self.response_sequence);
        while dequeue_count < rx_queue_count {
            let packet = self.network.rx_packets.as_queue_type_mut().pop_front().unwrap();
            trace!("{:?}", packet);
            match packet {
                Packet::Response{sequence: _, request_ack: _, code} => {
                    dequeue_count += 1;
                    self.response_sequence += 1;
                    self.process_event_code(code);
                }
                _ => panic!("Development bug: Non-response packet found in client RX queue")
            }
        }
    }

    fn process_event_code(&mut self, code: ResponseCode) {
        let conwayste_code = code.clone();
        if code != ResponseCode::OK && code != ResponseCode::KeepAlive {
            match self.conwayste_tx.send(conwayste_code) {
                Err(_) => error!("Could not send {:?} to conwayste", code.clone()),
                Ok(_) => ()
            }
        }
        match code {
            ResponseCode::OK => {
                match self.handle_response_ok() {
                    Ok(_) => {},
                    Err(e) => error!("{:?}", e)
                }
            }
            ResponseCode::LoggedIn(ref cookie, ref server_version) => {
                self.handle_logged_in(cookie.to_string(), server_version.to_string());
            }
            ResponseCode::LeaveRoom => {
                self.handle_left_room();
            }
            ResponseCode::JoinedRoom(ref room_name) => {
                self.handle_joined_room(room_name);
            }
            ResponseCode::PlayerList(ref player_names) => {
                self.handle_player_list(player_names.to_vec());
            }
            ResponseCode::RoomList(ref rooms) => {
                self.handle_room_list(rooms.to_vec());
            }
            ResponseCode::KeepAlive => {
                self.heartbeat = Some(Instant::now());
            },
            // errors
            _ => {
                error!("unknown response from server: {:?}", code);
            }
        }
    }

    fn handle_incoming_event(&mut self, udp_tx: &mpsc::UnboundedSender<(SocketAddr, Packet)>, opt_packet: Option<Packet>) {
        // All `None` packets should get filtered out up the hierarchy
        let packet = opt_packet.unwrap();
        match packet.clone() {
            Packet::Response{sequence, request_ack: _, code} => {
                self.process_event_code(ResponseCode::KeepAlive); // On any incoming event update the heartbeat.
                let code = code.clone();
                if code != ResponseCode::KeepAlive {
                    // When a packet is acked, we can remove it from the TX buffer and buffer the response for
                    // later processing.
                    // Removing a "Response packet" from the client's request TX buffer appears to be nonsense at first.
                    // This works because remove() targets different ID's depending on the Packet type. In the case of
                    // a Response packet, the target identifier is the `request_ack`.

                    // Only process responses we haven't seen
                    if self.response_sequence <= sequence {
                        trace!("RX Buffering: Resp.Seq.: {}, {:?}", self.response_sequence, packet);
                        // println!("TX packets: {:?}", self.network.tx_packets);
                        // None means the packet was not found so we've probably already removed it.
                        if let Some(_) = self.network.tx_packets.remove(&packet)
                        {
                            self.network.rx_packets.buffer_item(packet);
                        }

                        self.process_queued_server_responses();
                    }
                }
            }
            // TODO game_updates, universe_update
            Packet::Update{chats, game_updates: _, universe_update: _} => {
                self.handle_incoming_chats(chats);

                // Reply to the update
                let packet = Packet::UpdateReply {
                    cookie:               self.cookie.clone().unwrap(),
                    last_chat_seq:        Some(self.chat_msg_seq_num),
                    last_game_update_seq: None,
                    last_gen:             None,
                };

                netwayste_send!(udp_tx, (self.server_address.unwrap().clone(), packet),
                         ("Could not send UpdateReply{{ {} }} to server", self.chat_msg_seq_num));
            }
            Packet::Request{..} => {
                warn!("Ignoring packet from server normally sent by clients: {:?}", packet);
            }
            Packet::UpdateReply{..} => {
                warn!("Ignoring packet from server normally sent by clients: {:?}", packet);
            }
        }
    }

    fn handle_network_event(&mut self, udp_tx: &mpsc::UnboundedSender<(SocketAddr, Packet)>) {
        if self.cookie.is_some() {
            // Determine what can be processed
            // Determine what needs to be resent
            // Resend anything remaining in TX queue if it has also expired.
            self.process_queued_server_responses();

            let indices = self.network.tx_packets.get_retransmit_indices();

            self.network.retransmit_expired_tx_packets(udp_tx, self.server_address.unwrap().clone(), Some(self.response_sequence), &indices);
        }
    }

    fn handle_tick_event(&mut self, udp_tx: &mpsc::UnboundedSender<(SocketAddr, Packet)>) {
        // Every 100ms, after we've connected
        if self.cookie.is_some() {
            // Send a keep alive heartbeat if the connection is live
            let keep_alive = Packet::Request {
                cookie: self.cookie.clone(),
                sequence: self.sequence,
                response_ack: None,
                action: RequestAction::KeepAlive(self.response_sequence),
            };
            let timed_out = has_connection_timed_out(self.heartbeat);

            if timed_out || self.disconnect_initiated {
                if timed_out {
                    trace!("Server is non-responsive, disconnecting.");
                }
                if self.disconnect_initiated {
                    trace!("Disconnected from the server.")
                }
                self.reset();
            } else {
                netwayste_send!(udp_tx, (self.server_address.unwrap().clone(), keep_alive), ("Could not send KeepAlive packets"));
            }
        }

        self.tick = 1usize.wrapping_add(self.tick);
    }

    fn handle_user_input_event(&mut self,
            udp_tx: &mpsc::UnboundedSender<(SocketAddr, Packet)>,
            exit_tx: &mpsc::UnboundedSender<()>,
            user_input: UserInput) {

        let action;
        match user_input {
            UserInput::Chat(string) => {
                action = RequestAction::ChatMessage(string);
            }
            UserInput::Command{cmd, args} => {
                action = self.build_command_request_action(cmd, args);
            }
        }

        self.enqueue(udp_tx, exit_tx, action);
    }

    fn handle_response_ok(&mut self) -> Result<(), Box<Error>> {
            info!("OK :)");
            return Ok(());
    }

    fn handle_logged_in(&mut self, cookie: String, server_version: String) {
        self.cookie = Some(cookie);

        self.name = Some("blah3".to_owned()); //XXX HACK
        info!("Set client name to {:?}", self.name.clone().unwrap());
        self.check_for_upgrade(&server_version);
    }

    fn handle_joined_room(&mut self, room_name: &String) {
            self.room = Some(room_name.clone());
            info!("Joined room: {}", room_name);
    }

    fn handle_left_room(&mut self) {
        if self.in_game() {
            info!("Left room {}.", self.room.clone().unwrap());
        }
        self.room = None;
        self.chat_msg_seq_num = 0;
    }

    fn handle_player_list(&mut self, player_names: Vec<String>) {
        info!("---BEGIN PLAYER LIST---");
        for (i, player_name) in player_names.iter().enumerate() {
            info!("{}\tname: {}", i, player_name);
        }
        info!("---END PLAYER LIST---");
    }

    fn handle_room_list(&mut self, rooms: Vec<(String, u64, bool)>) {
        info!("---BEGIN GAME ROOM LIST---");
        for (game_name, num_players, game_running) in rooms {
            info!("#players: {},\trunning? {:?},\tname: {:?}",
                        num_players,
                        game_running,
                        game_name);
        }
        info!("---END GAME ROOM LIST---");
    }

    fn handle_incoming_chats(&mut self, chats: Option<Vec<BroadcastChatMessage>> ) {
        if let Some(mut chat_messages) = chats {
            chat_messages.retain(|ref chat_message| {
                self.chat_msg_seq_num < chat_message.chat_seq.unwrap()
            });
            // This loop does two things:
            //  1) update chat_msg_seq_num, and
            //  2) prints messages from other players
            for chat_message in chat_messages {
                let chat_seq = chat_message.chat_seq.unwrap();
                self.chat_msg_seq_num = std::cmp::max(chat_seq, self.chat_msg_seq_num);

                let queue = self.network.rx_chat_messages.as_mut().unwrap();
                queue.buffer_item(chat_message.clone());

                if let Some(ref client_name) = self.name.as_ref() {
                    if *client_name != &chat_message.player_name {
                        info!("{}: {}", chat_message.player_name, chat_message.message);
                    }
                } else {
                   panic!("Client name not set!");
                }
            }
        }
    }

    fn build_command_request_action(&mut self, cmd: String, args: Vec<String>) -> RequestAction {
        let mut action: RequestAction = RequestAction::None;
        // keep these in sync with print_help function
        match cmd.as_str() {
            "help" => {
                ClientNetState::print_help();
            }
            "stats" => {
                self.network.print_statistics();
            }
            "connect" => {
                if args.len() == 1 {
                    self.name = Some(args[0].clone());
                    action = RequestAction::Connect{
                        name:           args[0].clone(),
                        client_version: CLIENT_VERSION.to_owned(),
                    };
                } else { error!("Expected client name as the sole argument (no spaces allowed)."); }
            }
            "disconnect" => {
                if args.len() == 0 {
                    action = RequestAction::Disconnect;
                } else { debug!("Command failed: Expected no arguments to disconnect"); }
            }
            "list" => {
                if args.len() == 0 {
                    // players or rooms
                    if self.in_game() {
                        action = RequestAction::ListPlayers;
                    } else {
                        // lobby
                        action = RequestAction::ListRooms;
                    }
                } else { debug!("Command failed: Expected no arguments to list"); }
            }
            "new" => {
                if args.len() == 1 {
                    action = RequestAction::NewRoom(args[0].clone());
                } else { debug!("Command failed: Expected name of room (no spaces allowed)"); }
            }
            "join" => {
                if args.len() == 1 {
                    if !self.in_game() {
                        action = RequestAction::JoinRoom(args[0].clone());
                    } else {
                        debug!("Command failed: You are already in a game");
                    }
                } else { debug!("Command failed: Expected room name only (no spaces allowed)"); }
            }
            "part" | "leave" => {
                if args.len() == 0 {
                    if self.in_game() {
                        action = RequestAction::LeaveRoom;
                    } else {
                        debug!("Command failed: You are already in the lobby");
                    }
                } else { debug!("Command failed: Expected no arguments to leave"); }
            }
            "quit" => {
                trace!("Peace out!");
                action = RequestAction::Disconnect;
            }
            _ => {
                debug!("Command not recognized: {}", cmd);
            }
        }
        return action;
    }

    pub fn enqueue(&mut self,
            udp_tx: &mpsc::UnboundedSender<(SocketAddr, Packet)>,
            exit_tx: &mpsc::UnboundedSender<()>,
            action: RequestAction) {
        if action != RequestAction::None {
            // Sequence number can increment once we're talking to a server
            if self.cookie != None {
                self.sequence += 1;
            }

            let packet = Packet::Request {
                sequence:     self.sequence,
                response_ack: Some(self.response_sequence),
                cookie:       self.cookie.clone(),
                action:       action.clone(),
            };

            trace!("{:?}", packet);

            self.network.tx_packets.buffer_item(packet.clone());

            netwayste_send!(udp_tx, (self.server_address.unwrap().clone(), packet),
                            ("Could not send user input cmd to server"));

            if action == RequestAction::Disconnect {
                self.disconnect_initiated = true;
                netwayste_send!(exit_tx, ());
            }
        }
    }

    pub fn start_network(channel_to_conwayste: std::sync::mpsc::Sender<ResponseCode>,
                         channel_from_conwayste: mpsc::UnboundedReceiver<RequestAction>) {
        env_logger::Builder::new()
            .format(|buf, record| {
                writeln!(buf,
                    "{} [{:5}] - {}",
                    Local::now().format("%a %Y-%m-%d %H:%M:%S%.6f"),
                    record.level(),
                    record.args(),
                )
            })
            .filter(None, LevelFilter::Trace)
            .filter(Some("futures"), LevelFilter::Off)
            .filter(Some("tokio_core"), LevelFilter::Off)
            .filter(Some("tokio_reactor"), LevelFilter::Off)
            .init();

        let has_port_re = Regex::new(r":\d{1,5}$").unwrap(); // match a colon followed by number up to 5 digits (16-bit port)
        let mut server_str = env::args().nth(1).unwrap_or("localhost".to_owned());
        // if no port, add the default port
        if !has_port_re.is_match(&server_str) {
            debug!("Appending default port to {:?}", server_str);
            server_str = format!("{}:{}", server_str, DEFAULT_PORT);
        }

        // synchronously resolve DNS because... why not?
        trace!("Resolving {:?}...", server_str);
        let addr_vec = tokio_dns::resolve_sock_addr(&server_str[..]).wait()      // wait() is synchronous!!!
                        .unwrap_or_else(|e| {
                                error!("failed to resolve: {:?}", e);
                                exit(1);
                            });
        if addr_vec.len() == 0 {
            error!("resolution found 0 addresses");
            exit(1);
        }
        // TODO: support IPv6
        let addr_vec_len = addr_vec.len();
        let v4_addr_vec: Vec<_> = addr_vec.into_iter().filter(|addr| addr.is_ipv4()).collect(); // filter out IPv6
        if v4_addr_vec.len() < addr_vec_len {
            warn!("Filtered out {} IPv6 addresses -- IPv6 is not implemented.", addr_vec_len - v4_addr_vec.len() );
        }
        if v4_addr_vec.len() > 1 {
            // This is probably not the best option -- could pick based on ping time, random choice,
            // and could also try other ones on connection failure.
            warn!("Multiple ({:?}) addresses returned; arbitrarily picking the first one.", v4_addr_vec.len());
        }

        let addr = v4_addr_vec[0];

        trace!("Connecting to {:?}", addr);

        let mut core = Core::new().unwrap();
        let handle = core.handle();

        // Have separate thread read from stdin
        let (stdin_tx, stdin_rx) = mpsc::unbounded::<Vec<u8>>();
        let stdin_rx = stdin_rx.map_err(|_| panic!()); // errors not possible on rx

        // Unwrap ok because bind will abort if unsuccessful
        let udp = bind(&handle, Some("0.0.0.0"), Some(0)).unwrap();
        let local_addr = udp.local_addr().unwrap();

        // Channels
        let (udp_sink, udp_stream) = udp.framed(LineCodec).split();
        let (udp_tx, udp_rx) = mpsc::unbounded();    // create a channel because we can't pass the sink around everywhere
        let (exit_tx, exit_rx) = mpsc::unbounded();  // send () to exit_tx channel to quit the client

        trace!("Locally bound to {:?}.", local_addr);
        trace!("Will connect to remote {:?}.", addr);
        trace!("Type /help for more info...");

        // initialize state
        let mut initial_client_state = ClientNetState::new(channel_to_conwayste);
        initial_client_state.server_address = Some(addr);

        let iter_stream = stream::iter_ok::<_, io::Error>(iter::repeat( () )); // just a Stream that emits () forever
        // .and_then is like .map except that it processes returned Futures
        let tick_stream = iter_stream.and_then(|_| {
            let timeout = Timeout::new(Duration::from_millis(TICK_INTERVAL_IN_MS), &handle).unwrap();
            timeout.and_then(move |_| {
                ok(Event::TickEvent)
            })
        }).map_err(|e| {
            error!("Got error from tick stream: {:?}", e);
            exit(1);
        });

        let packet_stream = udp_stream
            .filter(|&(_, ref opt_packet)| {
                *opt_packet != None
            })
            .map(|packet_tuple| {
                Event::Incoming(packet_tuple)
            })
            .map_err(|e| {
                error!("Got error from packet_stream {:?}", e);
                exit(1);
            });

        let stdin_stream = stdin_rx
            .map(|buf| {
                let string = String::from_utf8(buf).unwrap();
                let string = String::from_str(string.trim()).unwrap();
                if !string.is_empty() && string != "" {
                    Some(string)
                } else {
                    None        // empty line; will be filtered out in next step
                }
            })
            .filter(|opt_string| {
                *opt_string != None
            })
            .map(|opt_string| {
                let string = opt_string.unwrap();
                let user_input = parse_stdin(string);
                Event::UserInputEvent(user_input)
            }).map_err(|_| ());

        let network_stream = stream::iter_ok::<_, io::Error>(iter::repeat( () ));
        let network_stream = network_stream.and_then(|_| {
            let timeout = Timeout::new(Duration::from_millis(NETWORK_INTERVAL_IN_MS), &handle).unwrap();
            timeout.and_then(move |_| {
                ok(Event::NetworkEvent)
            })
        }).map_err(|e| {
            error!("Got error from network_stream {:?}", e);
            exit(1);
        });

        let conwayste_rx = channel_from_conwayste.map_err(|_| panic!());
        let conwayste_stream = conwayste_rx.filter(|req| {
            *req != RequestAction::None
        })
        .map(|req| {
            Event::ConwaysteEvent(req)
        })
        .map_err(|e| {
            error!("Got error from conwayste event channel {:?}", e);
            exit(1);
        });


        let main_loop_fut = tick_stream
            .select(packet_stream)
            .select(stdin_stream)
            .select(network_stream)
            .select(conwayste_stream)
            .fold(initial_client_state, move |mut client_state: ClientNetState, event| {
                match event {
                    Event::Incoming((_addr, opt_packet)) => {
                        client_state.handle_incoming_event(&udp_tx, opt_packet);
                    }
                    Event::TickEvent => {
                        client_state.handle_tick_event(&udp_tx);
                    }
                    Event::UserInputEvent(user_input) => {
                        client_state.handle_user_input_event(&udp_tx, &exit_tx, user_input);
                    }
                    Event::NetworkEvent => {
                        client_state.handle_network_event(&udp_tx);
                    }
                    Event::ConwaysteEvent(request_action) => {
                        client_state.enqueue(&udp_tx, &exit_tx, request_action);
                    }
                }

                // finally, return the updated client state for the next iteration
                ok(client_state)
            })
            .map(|_| ())
            .map_err(|_| ());

        // listen on the channel created above and send it to the UDP sink
        let sink_fut = udp_rx.fold(udp_sink, |udp_sink, outgoing_item| {
            udp_sink.send(outgoing_item).map_err(|e| {
                    error!("Got error while attempting to send UDP packet: {:?}", e);
                    exit(1);
                })
        }).map(|_| ()).map_err(|_| ());

        let exit_fut = exit_rx
                        .into_future()
                        .map(|_| ())
                        .map_err(|e| {
                                    error!("Got error from exit_fut: {:?}", e);
                                    exit(1);
                                });

        let combined_fut = exit_fut
                            .select(main_loop_fut).map(|_| ()).map_err(|_| ())
                            .select(sink_fut).map_err(|_| ());

        thread::spawn(move || {
            read_stdin(stdin_tx);
        });
        drop(core.run(combined_fut).unwrap());
    }
}

// At this point we should only have command or chat message to work with
pub fn parse_stdin(mut input: String) -> UserInput {
    if input.get(0..1) == Some("/") {
        // this is a command
        input.remove(0);  // remove initial slash

        let mut words: Vec<String> = input.split_whitespace().map(|w| w.to_owned()).collect();

        let command = if words.len() > 0 {
                        words.remove(0).to_lowercase()
                    } else {
                        "".to_owned()
                    };

        UserInput::Command{cmd: command, args: words}
    } else {
            UserInput::Chat(input)
    }
}

// Our helper method which will read data from stdin and send it along the
// sender provided. This is blocking so should be on separate thread.
fn read_stdin(mut tx: mpsc::UnboundedSender<Vec<u8>>) {
    let mut stdin = io::stdin();
    loop {
        let mut buf = vec![0; 1024];
        let n = match stdin.read(&mut buf) {
            Err(_) |
            Ok(0) => break,
            Ok(n) => n,
        };
        buf.truncate(n);
        tx = match tx.send(buf).wait() {
            Ok(tx) => tx,
            Err(_) => break,
        };
    }
}

