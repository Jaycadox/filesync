# filesync
An easy to use cross-platform utility to send a file from one device to another via LAN.

## Usage
On the device which has the file, when attempting to send it to other devices:
```sh
filesync ./file
```

On the device that would like to recieve the file:

```sh
filesync
```

If all goes well, both programs will be able to detect eachother and negotiate a local file transfer.

## Notes

* The data that is sent is unencrypted/paintext. If one would want to send data in an encrypted fashion, they can use other tools to modify the file.
* The utility does not support password protection. A user can simply password protect an archive of the file before sending it, if wanted.

## Implementation details

`filesync` uses UDP broadcast packets for discovery. The server (which is the program which is sending the file), binds to a UDP socket and listens for "EOI" (Expression Of Interest) packets.

Once the server recieves an EOI packet, it binds a TCP socket then it sends an ACK (Acknowledgement Of Interest) packet directly to the client that expressed interest.

And once the client recieves an ACK, it attempts to connect to the TCP socket directly.

After the server and client have established a TCP connection, the server simply provides basic information about the file before starting the transfer.
* The name of the file
* The size of the file

The server then starts sending the raw data of the file to the client over TCP.
