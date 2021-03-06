extern crate alloc;
extern crate rcgen;
extern crate rustls;

use crate::grpc::Signature;
use prost::Message;
use rustls::{ Certificate,  RootCertStore, server::{AllowAnyAuthenticatedClient},  SignatureScheme, sign::{any_ecdsa_type, Signer}, internal::{msgs::handshake::DigitallySignedStruct}};
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::{cmp::Ordering, hash::{Hash, Hasher}, fmt::{self, Debug, Error, Formatter}, sync::Arc};
// apparently, ClientCertVerifier can't be imported for some reason?
// as a result, we have to recalculate AllowAnyAuthenticatedClient every time.

/// reflects default behaviour of rcgen's generated keys
pub const DEFAULT_SCHEME : SignatureScheme = SignatureScheme::ECDSA_NISTP256_SHA256;

/// Represents a Private Key in (with a string in PEM form)
/// Can be used to make digital signatures.
pub struct PrivateKey {
    string : String,
    signer : Box<dyn Signer>,
}
impl fmt::Display for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.string)
    }
}

/// Represents a corresponding public key (with a string in PEM form)
/// Can be used to verify digital signatures
#[derive(Clone)]
pub struct PublicKey {
    string : String,
    certificate : Certificate,
    scheme : SignatureScheme,
    // we could use a Box instead of an Arc, but then it wouldn't clone nicely.
    verify_closure : Arc<dyn Fn(&PublicKey, &[u8], Vec<u8>) -> bool + Send + Sync>,
}

impl fmt::Display for PublicKey {
    /// Public Keys print out as their PEM formatted String.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.string)
    }
}

impl PrivateKey {
    /// New private key using a crypto scheme from the slice given.
    /// panics if no valid crypto schemes for the given PEM key are in the slice.
    #[allow(dead_code)]
    pub fn new(pem_string : String, schemes : &[SignatureScheme]) -> PrivateKey {
      PrivateKey {
          string : pem_string.clone(),
          signer : any_ecdsa_type( &rustls::PrivateKey( pkcs8_private_keys(
                          &mut (pem_string.as_bytes())).expect(
                              &format!("pkcs8_private_keys could not parse {}", pem_string)
                          ).pop().expect(
                              &format!("pkcs8_private_keys found no keys in {}", pem_string)
                          ))).expect(
                              &format!("any_ecdsa_type found no ecdsa keys in {}", pem_string))
                     .choose_scheme(schemes).expect(
                          &format!("no valid schemes for {} in {:?}", pem_string, schemes)),
      }
    }
    /// new private key (from PEM encoding) using a specific crypto scheme
    /// panics if the crypto scheme given is not valid for this key.
    #[allow(dead_code)]
    pub fn new_specific_scheme(pem_string : String, scheme : SignatureScheme) -> PrivateKey {
        PrivateKey::new(pem_string, &vec![scheme])
    }
    /// new PrivateKey (from PEM encoding) using the default crypto scheme
    #[allow(dead_code)]
    pub fn new_default_scheme(pem_string : String) -> PrivateKey {
        PrivateKey::new_specific_scheme(pem_string, DEFAULT_SCHEME)
    }
    /// The crypto scheme being used here
    pub fn scheme(&self) -> SignatureScheme {
        self.signer.scheme()
    }
    /// make a digital signature for this array of bytes
    pub fn sign_bytes(&self, message : &[u8]) -> Vec<u8> {
        self.signer.sign(message).expect(
            &format!("Problem signing {:?} with key {}", message, self.string))
    }
    /// make a grpc::Signature out of a grpc Message
    pub fn sign_message(&self, message : &impl Message) -> Signature {
        Signature{ bytes : self.sign_bytes(&message.encode_to_vec()[..]) } 
    }
}

impl PublicKey {
    /// private method.
    /// Used to create a public key.
    /// This method will internally create a closure that is stored in the `PublicKey` object.
    /// That closure is called to verify signatures. 
    /// The closure is constructed using `verify_tls13_signature`. 
    /// It implicitly stores an object (opaque type) created with `AllowAnyAuthenticatedClient::new`.
    fn new_create_closure(pem_string : String, scheme : SignatureScheme, certificate : Certificate)
            -> PublicKey {
        let a = AllowAnyAuthenticatedClient::new(root_cert_store(&certificate));
        // I'd really like to just store `a` in the PublicKey struct, but its type is not
        // importable. Instead, I'm going to wrap it in a closure that I can store in a box.
        let verify_closure = move |s : &PublicKey, message : &[u8], signature|
            a.verify_tls13_signature(message,
                                     &s.certificate,
                                     &DigitallySignedStruct::new(s.scheme, signature)
                                    ).is_ok();
        PublicKey {
            certificate,
            string : pem_string,
            scheme,
            verify_closure : Arc::new(verify_closure),
        }
    }


    /// Make a new PublicKey using a PEM-encoded certificate and a specified Scheme.
    #[allow(dead_code)]
    pub fn new(pem_string : String, scheme : SignatureScheme) -> PublicKey {
        let c = certificate(&pem_string);
        PublicKey::new_create_closure(pem_string, scheme, c)
    }

    /// Make a new PublicKey using a PEM-encoded certificate (and the default Scheme)
    pub fn new_default_scheme(pem_string : String) -> PublicKey {
        let c = certificate(&pem_string);
        let mut schemes = supported_verify_schemes(&c);
        let complaint = format!("No signature schemes found for key {}", &pem_string);
        PublicKey::new_create_closure(pem_string,
                                      if schemes.contains(&DEFAULT_SCHEME) {
                                          DEFAULT_SCHEME
                                      } else {
                                          schemes.pop().expect(&complaint)
                                      },
                                      c)
    }

    /// Check that a digital signature was made with a private key corresponding to this public key
    pub fn verify_bytes(&self, message : &[u8], signature : Vec<u8>) -> bool {
        (self.verify_closure)(&self, message, signature)
    }
    /// Check that a grpc::Signature was made with a private key corresponding to this public key
    pub fn verify_signature(&self, message : &impl Message, signature : Signature) -> bool {
        self.verify_bytes(&message.encode_to_vec()[..], signature.bytes)
    }
}

impl Hash for PublicKey {
    /// We hash a `PublicKey` by hashing its `string` representation.
    fn hash<H: Hasher>(&self, state : &mut H) {
        self.string.hash(state)
    }
}

impl PartialEq for PublicKey {
    /// `PublicKey`s are equal if their `string` representations are equal.
    fn eq(&self, other : &Self) -> bool {
        self.string == other.string
    }
}

/// Given `PartialEq` is implemented, `Eq` is also implemented.
impl Eq for PublicKey {}


impl Ord for PublicKey {
    /// We order `PublicKey`s using the ordering on their `string` representations.
    fn cmp(&self, other : &Self) -> Ordering {
        self.string.cmp(&other.string)
    }
}

impl PartialOrd for PublicKey {
    /// We order `PublicKey`s using the ordering on their `string` representations.
    /// This directly uses the ordering from the `Ord` impl.
    fn partial_cmp(&self, other : &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Debug for PublicKey {
    /// We print out `PublicKey`s using their `string` (PEM-formetted) representation.
    fn fmt(&self, f : &mut Formatter<'_>) -> Result<(), Error> {
        self.string.fmt(f)
    }
}

/// Create a new `Certificate` given a PEM-formatted public key string.
/// Will panic and print out a line if it fails to parse. 
fn certificate(public_key_string : &String) -> Certificate {
    Certificate( certs(&mut (public_key_string.as_bytes())).expect(
                        &format!("could not parse certs from {}", &public_key_string)).pop()
        .expect(&format!("0 certs parsed from {}", &public_key_string)))
}

/// Create a `RootCertStore` featuring one `Certificate` as the only trusted root.
fn root_cert_store(certificate : &Certificate) -> RootCertStore {
    let mut root_cert_store = RootCertStore::empty();
    root_cert_store.add(certificate).unwrap();
    root_cert_store
}

/// Which `SignatureScheme`s does this `Certificate` support?
fn supported_verify_schemes(certificate : &Certificate) -> Vec<SignatureScheme> {
    AllowAnyAuthenticatedClient::new(root_cert_store(certificate))
        .supported_verify_schemes()
}


/// When possible, make a key pair with new_key_pair.
/// This ensures the signature schemes being used will match.
pub fn new_key_pair(hostnames : &[String]) -> (PublicKey, PrivateKey) {
    let cert = rcgen::generate_simple_self_signed(hostnames)
        .expect("Error while generating new key with rcgen.");
    keys_from_strings(
        cert.serialize_pem().expect("Error while marshaling public cert as PEM (using rcgen)."),
        cert.serialize_private_key_pem())
}

/// When possible, make a key pair with new_key_pair.
/// This ensures the signature schemes being used will match.
pub fn keys_from_strings(public_key_string: String, private_key_string: String)
        -> (PublicKey, PrivateKey) {
    let public_key = PublicKey::new_default_scheme(public_key_string);
    let private_key = PrivateKey::new_specific_scheme(private_key_string, public_key.scheme);
    (public_key, private_key)
}
