use core::format_args as f;
use embassy_net::IpAddress;
use embassy_rp::{peripherals::UART0, pio, pio_programs::ws2812::PioWs2812};
use smart_leds::RGB8;

use crate::{UartWriter, PIXEL_COUNT};

pub async fn receive_artnet<P: pio::Instance>(
    s: &mut UartWriter<'_, UART0>,
    stack: embassy_net::Stack<'static>,
    mut strip: PioWs2812<'_, P, 0, PIXEL_COUNT>,
) {
    use embassy_net::udp::{PacketMetadata, UdpSocket};

    s.println(f!("Connected!"));
    let address_bytes = stack.config_v4().unwrap().address.address().octets();
    s.println(f!(
        "IP Address: {}.{}.{}.{}",
        address_bytes[0],
        address_bytes[1],
        address_bytes[2],
        address_bytes[3]
    ));
    let hardware_address = stack.hardware_address();
    let hardware_address_bytes = hardware_address.as_bytes();
    s.println(f!(
        "MAC Address: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        hardware_address_bytes[0],
        hardware_address_bytes[1],
        hardware_address_bytes[2],
        hardware_address_bytes[3],
        hardware_address_bytes[4],
        hardware_address_bytes[5]
    ));

    let mut rx_meta = [PacketMetadata::EMPTY; 16];
    // let mut rx_buffer = [0; 120000];
    let mut rx_buffer = [0; 1024];
    let mut tx_meta = [PacketMetadata::EMPTY; 16];
    let mut tx_buffer = [0; 1024];
    // let mut buf = [0; 65507];
    let mut buf = [0; 1024];

    let mut socket = UdpSocket::new(
        stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );
    socket.bind(tiny_artnet::PORT).unwrap();
    s.println(f!("Artnet port {} bound", tiny_artnet::PORT));

    // pixels_0[0] = RGB8::new(255, 0, 255);
    // strip.write(pixels_0).await;

    // let mut last_sequence: u8 = 0; // DEBUG
    loop {
        let (packet_length, metadata) = socket.recv_from(&mut buf).await.unwrap();

        // s.print(f!("Received a packet of length {}", packet_length));

        // if packet_length >= 8 {
        //     s.println(f!(
        //         " with these first 8 bytes: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
        //         buf[0],
        //         buf[1],
        //         buf[2],
        //         buf[3],
        //         buf[4],
        //         buf[5],
        //         buf[6],
        //         buf[7]));
        // } else {
        //     s.println(f!(" which is less than 8 bytes long"));
        // }

        // let data_size: usize = 512;
        let pixels_per_universe: usize = 512 / 3;
        let mut pixels_0 = [RGB8::default(); PIXEL_COUNT];
        // let mut pixels_1 = [0u32; PIXEL_COUNT];
        // let mut pixels_2 = [0u32; PIXEL_COUNT];
        // let mut pixels_3 = [0u32; PIXEL_COUNT];
        match tiny_artnet::from_slice(&buf[..packet_length]) {
            Ok(tiny_artnet::Art::Dmx(dmx)) => {
                //s.println(f!("received artnet: dmx"));

                // The dmx.port_address.as_index() calculation is wrong so I am doing my own.
                // let port_address = dmx.port_address.as_index();
                let port_address = ((dmx.port_address.net as usize) << 8)
                    + ((dmx.port_address.sub_net as usize) << 4)
                    + (dmx.port_address.universe as usize);

                // DEBUG
                // let sequence = dmx.sequence;
                // if sequence != last_sequence.wrapping_add(1) {
                //     let mut str: heapless::String<24> = heapless::String::new();
                //     core::write!(
                //         &mut str,
                //         "seq: {} ; skipped: {}\r\n",
                //         sequence,
                //         sequence - last_sequence + 1
                //     )
                //     .unwrap();
                //     s.print(f!(str.as_str()));
                // }
                // last_sequence = sequence;

                // s.print(f!("port address: "));
                // let mut port_address_string: heapless::String<24> = heapless::String::new();
                // core::write!(
                //     &mut port_address_string,
                //     "{} ({}, {}, {})",
                //     port_address,
                //     dmx.port_address.net,
                //     dmx.port_address.sub_net,
                //     dmx.port_address.universe
                // )
                // .unwrap();
                // s.print(f!(&port_address_string.as_str()));
                // s.print(f!("\r\n"));

                let start_of_universe_in_pixel_array = (port_address % 10) * pixels_per_universe;
                // s.print(f!("3\r\n"));
                let data_iter = dmx
                    .data
                    .chunks_exact(3)
                    .take((PIXEL_COUNT - start_of_universe_in_pixel_array).max(0))
                    .enumerate();
                if port_address < 10 {
                    data_iter.for_each(|(i, pixel)| {
                        pixels_0[start_of_universe_in_pixel_array + i] =
                            RGB8::new(pixel[0], pixel[1], pixel[2]);
                        // (u32::from(pixel[0]) << 16) | (u32::from(pixel[1]) << 24) | (u32::from(pixel[2]) << 8);
                    });
                    if start_of_universe_in_pixel_array == 0 {
                        strip.write(&pixels_0).await;
                    }

                    // data_iter.for_each(|(i, pixel)| {
                    //     pixels_0[start_of_universe_in_pixel_array + i] =
                    //         // RGB8::new(pixel[0], pixel[1], pixel[2]);
                    //         (u32::from(pixel[0]) << 16) | (u32::from(pixel[1]) << 24) | (u32::from(pixel[2]) << 8);
                    // });
                    // if start_of_universe_in_pixel_array == 0 {
                    //     strip.write_direct(&pixels_0).await;
                    // }

                    // } else if port_address < 20 {
                    //     data_iter.for_each(|(i, pixel)| {
                    //         pixels_1[start_of_universe_in_pixel_array + i] =
                    //             // RGB8::new(pixel[0], pixel[1], pixel[2]);
                    //             (u32::from(pixel[0]) << 16) | (u32::from(pixel[1]) << 24) | (u32::from(pixel[2]) << 8);
                    //     });
                    //     if start_of_universe_in_pixel_array == 0 {
                    //         strip_1.write_direct(&pixels_1).await;
                    //     }
                    // } else if port_address < 30 {
                    //     data_iter.for_each(|(i, pixel)| {
                    //         pixels_2[start_of_universe_in_pixel_array + i] =
                    //             // RGB8::new(pixel[0], pixel[1], pixel[2]);
                    //             (u32::from(pixel[0]) << 16) | (u32::from(pixel[1]) << 24) | (u32::from(pixel[2]) << 8);
                    //     });
                    //     if start_of_universe_in_pixel_array == 0 {
                    //         strip_2.write_direct(&pixels_2).await;
                    //     }
                    // } else if port_address < 40 {
                    //     data_iter.for_each(|(i, pixel)| {
                    //         pixels_3[start_of_universe_in_pixel_array + i] =
                    //             // RGB8::new(pixel[0], pixel[1], pixel[2]);
                    //             (u32::from(pixel[0]) << 16) | (u32::from(pixel[1]) << 24) | (u32::from(pixel[2]) << 8);
                    //     });
                    //     if start_of_universe_in_pixel_array == 0 {
                    //         strip_3.write_direct(&pixels_3).await;
                    //     }
                }
            }
            Ok(tiny_artnet::Art::Poll(_poll)) => {
                s.println(f!("received artnet: poll"));
                let reply = tiny_artnet::PollReply {
                    ip_address: &address_bytes,
                    port: tiny_artnet::PORT,
                    mac_address: &{
                        let mut a = [0u8; 6];
                        a.iter_mut()
                            .zip(hardware_address_bytes.iter())
                            .for_each(|(a, b)| *a = *b);
                        a
                    },
                    ..Default::default()
                };

                let poll_reply_len = reply.serialize(&mut buf);

                s.print(f!("sending reply of length {}", poll_reply_len));
                let IpAddress::Ipv4(x) = metadata.endpoint.addr;
                s.println(f!(
                    " to {}.{}.{}.{}:{}",
                    x.octets()[0],
                    x.octets()[1],
                    x.octets()[2],
                    x.octets()[3],
                    metadata.endpoint.port
                ));

                match socket
                    .send_to(&buf[..poll_reply_len], metadata.endpoint)
                    .await
                {
                    Ok(_) => {
                        s.println(f!("sent poll reply"));
                    }
                    Err(_) => {
                        s.println(f!("error sending poll reply"));
                    }
                }
            }
            Ok(tiny_artnet::Art::Command(_)) => {
                s.println(f!("received artnet: command"));
            }
            Ok(tiny_artnet::Art::Sync) => {
                s.println(f!("received artnet: sync"));
            }
            Err(_) => {
                s.println(f!("received artnet: error"));
            }
        }
    }
}
