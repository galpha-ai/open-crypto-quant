Title: WSS Overview - Polymarket Documentation

Websocket

WSS Overview
============

Overview and general information about the Polymarket Websocket

Overview
------------------------------------------------------------------------------------------

The Polymarket CLOB API provides websocket (wss) channels through which clients can get pushed updates. These endpoints allow clients to maintain almost real-time views of their orders, their trades and markets in general. There are two available channels `user` and `market`.

Subscription
--------------------------------------------------------------------------------------------------

To subscribe send a message including the following authentication and intent information upon opening the connection.

| Field      | Type     | Description                                                                 |
|------------|----------|-----------------------------------------------------------------------------|
| auth       | Auth     | see next page for auth information                                          |
| markets    | string[] | array of markets (condition IDs) to receive events for (for `user` channel) |
| assets_ids | string[] | array of asset ids (token IDs) to receive events for (for `market` channel) |
| type       | string   | id of channel to subscribe to (USER or MARKET)                              |

Where the `auth` field is of type `Auth` which has the form described in the WSS Authentication section below.
