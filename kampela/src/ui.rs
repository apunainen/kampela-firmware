//! Everything high-level related to interfacing with user

use nalgebra::{Affine2, OMatrix, Point2, RowVector3};
use alloc::{collections::VecDeque, string::String, vec::Vec};
use lazy_static::lazy_static;
use substrate_crypto_light::sr25519::{Pair, Public};
use embedded_graphics::{
    prelude::Point,
    geometry::Dimensions,
};

use kampela_system::{
    devices::{
        psram::{psram_decode_call, psram_decode_extension, read_from_psram, PsramAccess},
        se_aes_gcm::{decode_entropy, encode_entropy, Protected},
        se_rng,
        touch::{touch_detected, Read, FT6X36_REG_NUM_TOUCHES, LEN_NUM_TOUCHES}
    }, draw::FrameBuffer, flash_mnemonic::FlashWordList, parallel::Operation
};
use kampela_system::devices::flash::*;
use crate::nfc::NfcTransactionPsramAccess;
use kampela_ui::{
    display_def::*,
    platform::{PinCode, Platform},
    uistate::{UIState, UpdateRequest, UpdateRequestMutate}
};

const MAX_TOUCH_QUEUE: usize = 2;

/// UI handler
pub struct UI {
    pub state: UIState<Hardware, FrameBuffer>,
    status: UIStatus,
    touches: VecDeque<Point>,
    touched: bool,
    update_request: Option<UpdateRequest>,
}

impl UI {
    /// Start of UI.
    pub fn init() -> Self {
        let hardware = Hardware::new();
        let display = FrameBuffer::new_white();
        let state = UIState::new(hardware, display, &mut ());
        return Self {
            state,
            status: UIStatus::DisplayOrListen(UIStatusDisplay::Listen),
            touches: VecDeque::new(),
            touched: false,
            update_request: Some(UpdateRequest::Slow),
        }
    }

    /// Call in event loop to progress through UI state
    pub fn advance(&mut self, voltage: i32) -> Option<bool> {
        match self.status {
            UIStatus::DisplayOrListen(ref mut status) => {
                // read input if possible
                if touch_detected().unwrap_or(false) {
                    if self.touched == false && !matches!(status, UIStatusDisplay::DisplayOperation(UpdateRequest::Slow)) {
                        self.touched = true;
                        self.status = UIStatus::TouchOperation(Read::new(()), core::mem::take( status));
                        return None
                    }
                } else {
                    self.touched = false;
                }
                match status {
                    UIStatusDisplay::Listen => {
                        self.listen();
                        Some(true) // done operations
                    },
                    UIStatusDisplay::DisplayOperation(_) => {
                        match self.state.display.advance(voltage) {
                            Some(c) => {
                                if c {
                                    self.status = UIStatus::DisplayOrListen(UIStatusDisplay::Listen);
                                }
                                Some(false)
                            },
                            None => None, // not enough energy to start screen update
                        }
                    },
                }
            }
            UIStatus::TouchOperation(ref mut touch, ref mut next) => {
                match touch.advance(()) {
                    Ok(Some(touch)) => {
                        if self.touches.len() < MAX_TOUCH_QUEUE {
                            if let Some(point) = convert(touch) {
                                self.touches.push_back(point);
                            }
                        }
                        self.status = UIStatus::DisplayOrListen(core::mem::take(next));
                        None
                    },
                    Ok(None) => {None},
                    Err(e) => panic!("{:?}", e),
                }
            },
        }
    }

    fn listen(&mut self) {
        if let Some(point) = self.touches.pop_front() {
            self.update_request.propagate(self.state.handle_tap(point, &mut ()));
        }
        // update ui if needed
        if let Some(u) = self.update_request.take() {
            let is_clear_update = matches!(u, UpdateRequest::Slow) || matches!(u, UpdateRequest::Fast);
            self.update_request.propagate(self.state.render(is_clear_update, &mut ()).expect("guaranteed to work, no errors implemented"));

            match u {
                UpdateRequest::Hidden => (),
                UpdateRequest::Slow => self.state.display.request_full(),
                UpdateRequest::Fast => self.state.display.request_fast(),
                UpdateRequest::UltraFast => {
                    let a = self.state.display.bounding_box();
                    self.state.display.request_part(a);
                },
                UpdateRequest::Part(a) => self.state.display.request_part(a),
            }
            if !matches!(u, UpdateRequest::Hidden) {
                self.status = UIStatus::DisplayOrListen(UIStatusDisplay::DisplayOperation(u));
            }
        }
    }

    pub fn handle_message(&mut self, message: String) {
        self.update_request.propagate(self.state.handle_message(message, &mut ()));
    }

    pub fn handle_transaction(&mut self, transaction: NfcTransactionPsramAccess) {
        self.state.platform.set_transaction(transaction);
        self.update_request.propagate(self.state.handle_transaction(&mut ()));
    }

    pub fn handle_address(&mut self, addr: [u8; 76]) {
        self.update_request.propagate(self.state.handle_address(addr));
    }
}

/// General status of UI
///
/// There is no sense in reading input while screen processes last event, nor refreshing the screen
/// before touch was parsed

enum UIStatusDisplay {
    /// Event listening state, default
    Listen,
    /// Screen update started
    DisplayOperation(UpdateRequest),
}
impl Default for UIStatusDisplay {
    fn default() -> Self { UIStatusDisplay::Listen }
}
enum UIStatus {
    DisplayOrListen(UIStatusDisplay),
    /// Touch event processing
    TouchOperation(Read<LEN_NUM_TOUCHES, FT6X36_REG_NUM_TOUCHES>, UIStatusDisplay),
}
pub struct Hardware {
    pin: PinCode,
    protected: Option<Protected>,
    address: Option<[u8; 76]>,
    transaction_psram_access: Option<NfcTransactionPsramAccess>,
}

impl Hardware {
    pub fn new() -> Self {
        let protected = None;
        let pin_set = false; // TODO query storage
        let pin = [0; 4];
        Self {
            pin,
            protected,
            address: None,
            transaction_psram_access: None,
        }
    }
}

impl Platform for Hardware {
    type HAL = ();
    type Rng<'c> = se_rng::SeRng;
    type AsWordList = FlashWordList;

    type NfcTransaction = NfcTransactionPsramAccess;
    fn get_wordlist() -> Self::AsWordList {
        FlashWordList::new()
    }

    fn rng<'b>(_: &'b mut ()) -> Self::Rng<'static> {
        se_rng::SeRng{}
    }

    fn pin(&self) -> &PinCode {
        &self.pin
    }

    fn pin_mut(&mut self) -> &mut PinCode {
        &mut self.pin
    }

    fn store_entropy(&mut self, e: &[u8]) {
        self.protected = if e.len() != 0 {
            let protected = encode_entropy(e);
            store_encoded_entopy(&protected);
            Some(protected)
        } else {
            None
        }
    }

    fn read_entropy(&mut self) {
        self.protected = read_encoded_entropy();
    }

    fn public(&self) -> Option<Public> {
        if let Some(e) = self.entropy() {
            Some(Pair::from_entropy_and_pwd(&e, "").unwrap().public())
        } else {
            None
        }
    }

    fn entropy(&self) -> Option<Vec<u8>> {
        if let Some(p) = &self.protected {
            Some(decode_entropy(&p))
        } else {
            None
        }
    }

    fn set_address(&mut self, addr: [u8; 76]) {
        self.address = Some(addr);
    }

    fn set_transaction(&mut self, transaction: Self::NfcTransaction) {
        self.transaction_psram_access = Some(transaction);
    }


    fn call(&mut self) -> Option<String> {
        let transaction_psram_access = match self.transaction_psram_access {
            Some(ref a) => a,
            None => return None
        };

        let (decoded_call, specs, spec_name) = psram_decode_call(
            &transaction_psram_access.call_psram_access,
            &transaction_psram_access.metadata_psram_access,
        );

        let carded = decoded_call.card(0, &specs, &spec_name);
        let call = carded
            .into_iter()
            .map(|card| card.show())
            .collect::<Vec<String>>()
            .join("\n");

        Some(call)
    }

    fn extensions(&mut self) -> Option<String> {
        let transaction_psram_access = match self.transaction_psram_access {
            Some(ref a) => a,
            None => return None
        };
        
        let (decoded_extension, specs, spec_name) = psram_decode_extension(
            &transaction_psram_access.extension_psram_access,
            &transaction_psram_access.metadata_psram_access,
            &transaction_psram_access.genesis_hash_bytes_psram_access
        );

        let mut carded = Vec::new();
        for ext in decoded_extension.iter() {
            let addition_set = ext.card(0, true, &specs, &spec_name);
            if !addition_set.is_empty() {
                carded.extend_from_slice(&addition_set)
            }
        }
        let extensions = carded
            .into_iter()
            .map(|card| card.show())
            .collect::<Vec<String>>()
            .join("\n");

        Some(extensions)
    }

    fn signature(&mut self) -> [u8; 130] {
        let transaction_psram_access = match self.transaction_psram_access {
            Some(ref a) => a,
            None => panic!("qr generation failed")
        };
        
        let data_to_sign_psram_access = PsramAccess {
            start_address: transaction_psram_access.call_psram_access.start_address,
            total_len:
                transaction_psram_access.call_psram_access.total_len
                + &transaction_psram_access.extension_psram_access.total_len
        };
        let data_to_sign = read_from_psram(&data_to_sign_psram_access);

        let signature = self.pair()
            .expect("entropy should be stored at this point")
            .sign_external_rng(&data_to_sign, &mut Self::rng(&mut ()));

        let mut signature_with_id: [u8; 65] = [1; 65];
        signature_with_id[1..].copy_from_slice(&signature.0);
        let signature_with_id_bytes = hex::encode(signature_with_id)
            .into_bytes()
            .try_into()
            .expect("static length");

        signature_with_id_bytes
    }

    fn address(&mut self) -> &[u8; 76] {
        if let Some(ref a) = self.address {
            a
        } else {
            panic!("qr generation failed");
        }
    }

}

lazy_static! {
    // MAGIC calibration numbers obtained through KOLIBRI tool
    static ref AFFINE_MATRIX: Affine2<f32> = Affine2::from_matrix_unchecked(
        OMatrix::from_rows(&[
            RowVector3::<f32>::new(1.0022, -0.0216, -4.2725),
            RowVector3::<f32>::new(0.0061, 1.1433, -13.7305),
            RowVector3::<f32>::new(0.0, 0.0, 1.0),
        ])
    );
}



pub fn convert(touch_data: [u8; LEN_NUM_TOUCHES]) -> Option<Point> {
    if touch_data[0] == 1 {
        let detected_y = (((touch_data[1] as u16 & 0b00001111) << 8) | touch_data[2] as u16) as i32;
        let detected_x = (((touch_data[3] as u16 & 0b00001111) << 8) | touch_data[4] as u16) as i32;
        let touch = Point::new(SCREEN_SIZE_X as i32 - detected_x, detected_y);

        let touch_as_point2 = Point2::new(touch.x as f32, touch.y as f32);
        let display_as_point2 = AFFINE_MATRIX.transform_point(&touch_as_point2);

        Some(
            Point {
                x: display_as_point2.coords[0] as i32,
                y: display_as_point2.coords[1] as i32,
            }
        )
    } else { None }
}


