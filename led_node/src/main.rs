#![no_std]
#![no_main]

mod artnet;
mod pixel_control;
use core::format_args as f;
use defmt_serial as _;
use embassy_executor::Spawner;
use embassy_net::{Config as NetConfig, Ipv4Address, Ipv4Cidr, Stack, StackResources};
use embassy_net_wiznet::Runner;
use embassy_rp::{
    bind_interrupts,
    gpio::{Input, Level, Output, Pull},
    peripherals::{BOOTSEL, PIO0, PIO1, SPI0, UART0},
    pio::{self, Pio},
    pio_programs::ws2812::{PioWs2812, PioWs2812Program},
    spi::{Async, Config as SpiConfig, Spi},
    uart,
};
use embassy_time::{Delay, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;
use panic_probe as _;
use smart_leds::RGB8;
use static_cell::StaticCell;

// CONFIG
const PIXEL_COUNT: usize = 8;
const IP_ADDRESS_SECOND_TO_LAST_NUMBER: u8 = 5;
const IP_ADDRESS_LAST_NUMBER: u8 = 51;
// 169.254.9.91-99
// 169.254.5.51

// Needed for tiny-artnet
#[global_allocator]
static HEAP: embedded_alloc::LlffHeap = embedded_alloc::LlffHeap::empty();

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
    PIO1_IRQ_0 => pio::InterruptHandler<PIO1>;
});

#[embassy_executor::task]
async fn bootsel_task(mut bootsel: BOOTSEL) -> ! {
    loop {
        checkbootsel(&mut bootsel);
        Timer::after_secs(1).await;
    }
}
fn checkbootsel(bootsel: &mut BOOTSEL) {
    if bootsel.is_pressed() {
        embassy_rp::rom_data::reset_to_usb_boot(0, 0);
    }
}

struct UartWriter<'d, T: uart::Instance>(uart::Uart<'d, T, uart::Blocking>);
#[allow(dead_code)]
impl<T: uart::Instance> UartWriter<'_, T> {
    fn println(&mut self, args: core::fmt::Arguments) {
        let mut s: heapless::String<64> = heapless::String::new();
        let _ = core::fmt::write(&mut s, format_args!("{args}\n"));

        let _ = self.0.blocking_write(s.as_bytes());
        let _ = self.0.blocking_flush();
    }
    fn print(&mut self, args: core::fmt::Arguments) {
        let mut s: heapless::String<64> = heapless::String::new();
        let _ = core::fmt::write(&mut s, args);

        let _ = self.0.blocking_write(s.as_bytes());
        let _ = self.0.blocking_flush();
    }
}

#[embassy_executor::task]
#[allow(clippy::type_complexity)]
async fn ethernet_task(
    runner: Runner<
        'static,
        embassy_net_wiznet::chip::W5500,
        ExclusiveDevice<Spi<'static, SPI0, Async>, Output<'static>, Delay>,
        Input<'static>,
        Output<'static>,
    >,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]

async fn net_task(
    mut runner: embassy_net::Runner<'static, embassy_net_wiznet::Device<'static>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // Set up task for entering boot mode when holding bootsel button
    let _ = spawner.spawn(bootsel_task(p.BOOTSEL));

    // Set up serial printing
    static SERIAL: StaticCell<UartWriter<'_, UART0>> = StaticCell::new();
    let s = SERIAL.init_with(|| {
        UartWriter(uart::Uart::new_blocking(
            p.UART0,
            p.PIN_0,
            p.PIN_1,
            uart::Config::default(),
        ))
    });

    // Set up pixel control
    let mut pio_neopixel_0 = Pio::new(p.PIO1, Irqs);

    let program = PioWs2812Program::new(&mut pio_neopixel_0.common);
    let mut pc: PioWs2812<'_, _, 0, PIXEL_COUNT> = PioWs2812::new(
        &mut pio_neopixel_0.common,
        pio_neopixel_0.sm0,
        p.DMA_CH4,
        p.PIN_6,
        &program,
    );
    // let mut strip0: PioWs2812<'_, _, 0, PIXEL_COUNT> = PioWs2812::new(
    //     &mut pio_neopixel_0.common,
    //     pio_neopixel_0.sm0,
    //     p.DMA_CH4,
    //     p.PIN_6,
    //     &program,
    // );
    // let mut strip1: PioWs2812<'_, _, 0, PIXEL_COUNT> = PioWs2812::new(
    //     &mut pio_neopixel_0.common,
    //     pio_neopixel_0.sm1,
    //     p.DMA_CH5,
    //     p.PIN_7,
    //     &program,
    // );
    // let mut strip2: PioWs2812<'_, _, 0, PIXEL_COUNT> = PioWs2812::new(
    //     &mut pio_neopixel_0.common,
    //     pio_neopixel_0.sm2,
    //     p.DMA_CH6,
    //     p.PIN_8,
    //     &program,
    // );
    // let mut strip3: PioWs2812<'_, _, 0, PIXEL_COUNT> = PioWs2812::new(
    //     &mut pio_neopixel_0.common,
    //     pio_neopixel_0.sm3,
    //     p.DMA_CH7,
    //     p.PIN_9,
    //     &program,
    // );

    let mut pixels = [RGB8::default(); PIXEL_COUNT];
    for i in &mut pixels {
        i.r = 128;
    }
    pc.write(&pixels).await;

    // Connct to w5500 peripheral
    let mut spi_cfg = SpiConfig::default();
    spi_cfg.frequency = 50_000_000;
    let (miso, mosi, clk) = (p.PIN_16, p.PIN_19, p.PIN_18);
    let spi = Spi::new(p.SPI0, clk, mosi, miso, p.DMA_CH0, p.DMA_CH1, spi_cfg);
    let cs = Output::new(p.PIN_17, Level::High);
    let w5500_int = Input::new(p.PIN_21, Pull::Up);
    let w5500_reset = Output::new(p.PIN_20, Level::High);

    // Set up ethernet task
    let mac_addr = [0x00, 0x00, 0x00, 0x00, 0x00, IP_ADDRESS_LAST_NUMBER];
    static STATE: StaticCell<embassy_net_wiznet::State<8, 8>> = StaticCell::new();
    let state = STATE.init(embassy_net_wiznet::State::<8, 8>::new());
    let (w5500_device, ethernet_task_runner) = embassy_net_wiznet::new(
        mac_addr,
        state,
        ExclusiveDevice::new(spi, cs, Delay),
        w5500_int,
        w5500_reset,
    )
    .await
    .unwrap();
    spawner.spawn(ethernet_task(ethernet_task_runner)).unwrap();

    for i in &mut pixels {
        i.r = 0;
        i.g = 128;
    }
    pc.write(&pixels).await;

    // Set up network stack
    let static_ip_net_config = NetConfig::ipv4_static(embassy_net::StaticConfigV4 {
        // Direct/unmanaged ethernet such as with switch GS308
        address: Ipv4Cidr::new(
            Ipv4Address::new(
                169,
                254,
                IP_ADDRESS_SECOND_TO_LAST_NUMBER,
                IP_ADDRESS_LAST_NUMBER,
            ),
            16,
        ),
        // Managed ethernet switch GS308T
        // address: Ipv4Cidr::new(Ipv4Address::new(192, 168, 11, IP_ADDRESS_LAST_NUMBER), 24),
        dns_servers: heapless::Vec::new(),
        gateway: None,
    });

    // let dhcp_net_config = NetConfig::dhcpv4(Default::default());
    static STACK_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
    let seed = 0xafd4_37bc_79fd_c225;
    let (stack, net_task_runner) = embassy_net::new(
        w5500_device,
        static_ip_net_config,
        STACK_RESOURCES.init(StackResources::new()),
        seed,
    );
    spawner.spawn(net_task(net_task_runner)).unwrap();

    for i in &mut pixels {
        i.g = 0;
        i.b = 128;
    }
    pc.write(&pixels).await;

    async fn wait_for_config(stack: Stack<'static>) -> embassy_net::StaticConfigV4 {
        use embassy_futures::yield_now;

        loop {
            if let Some(config) = stack.config_v4() {
                return config.clone();
            }
            yield_now().await;
        }
    }
    s.println(f!("waiting for stack config..."));
    wait_for_config(stack).await;
    s.println(f!("connected!"));

    for i in &mut pixels {
        i.r = 255;
        i.g = 255;
        i.b = 255;
    }
    pc.write(&pixels).await;

    artnet::receive_artnet(s, stack, pc).await;

    // let delay = Duration::from_secs(1);
    // loop {
    //     checkbootsel(&mut p.BOOTSEL);
    //     s.println(f!("led on!"));
    //     cyw43_control.gpio_set(0, true).await;
    //     Timer::after(delay).await;

    //     checkbootsel(&mut p.BOOTSEL);
    //     s.println(f!("led off!"));
    //     cyw43_control.gpio_set(0, false).await;
    //     Timer::after(delay).await;
    // }
}
