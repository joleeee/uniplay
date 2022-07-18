Uniplay uses mqtt for the underlying transport. It usually runs on top of
TCP/IP, but it gives additional features which lets us focus on implementing
The Service, instead of dealing with transport layer issues.

MQTT is quite simple, and an upshot of using this protocol is that we can leech
off of public brokers. At a later date The Service may be available over
similar protocols, perhaps a proper P2P one.

The goal of Uniplay is to *Syncronize playback*. This is done by sharing state.
The Protocol assumes all actors are following The Protocol faithfully and are
well-intentioned. A sufficiently hard-to-guess room name should provide
protection against bad actors.

This document will be a high-level overview. It will not specify how data is
encoded, but rather explain the abstract control structures.

# Joining a room
When joining a room, the joinee sends a Join message with its name. Everyone
then immediately responds with a ping as described below.

# Participation
Every participant should send a ping every 5s, or whenever an action is taken.
A ping consists of the current
1. video path
2. seek position
3. date
4. status

# Parting a room
Currently parting a room does not require and messages, others will simply
assume you left when you stop sending pings for a while.

# Sending a message
End users can communicate by sending messages. A message consists of a sender
and the content.
