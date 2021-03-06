pub mod acceptor;
pub mod crypto;
pub mod learner;
pub mod parse_config;
pub mod parsed_message;
pub mod proposer;

/// Include generated code from `proto/hetpaxosref.proto`.
/// This will include serde serialization code for JSON compatibility.
/// Also adds a simple `is_one_a` method to `ConsensusMessage`s.
pub mod grpc {
    tonic::include_proto!("hetpaxosref");
    include!(concat!(env!("OUT_DIR"), "/hetpaxosref.serde.rs"));

    impl ConsensusMessage {
        /// quick check to see if this is a one_a message: does it have a ballot with a timestamp
        /// and hash_value?
        pub fn is_one_a(&self) -> bool {
            match self {
                ConsensusMessage{message_oneof : Some(consensus_message::MessageOneof::Ballot(
                        Ballot{timestamp : Some(_), value_hash : Some(_)}))} => true,
                _ => false,
            }
        }
    }
}

/// Include generated code from `proto/hetpaxosrefconfig.proto`.
/// This will include serde serialization code for JSON compatibility.
pub mod config {
    tonic::include_proto!("hetpaxosrefconfig");
    include!(concat!(env!("OUT_DIR"), "/hetpaxosrefconfig.serde.rs"));
}

/// Utility functions used in several places in `het_paxos_ref`.
pub mod utils {
    use byteorder::{BigEndian, ByteOrder};
    use crate::grpc::{Ballot, Hash256};
    use pbjson_types::Timestamp;
    use prost:: Message;
    use sha3::{Digest, Sha3_256};
    use std::{fmt::{Display, Formatter, Result}, hash::{Hash, Hasher}, cmp::Ordering};

    /// Hash a protobuf Message struct with Sha3 into a Hash256 struct.
    /// Bytes marshaled in BigEndian order.
    pub fn hash(message : &impl Message) -> Hash256 {
        let bytes = Sha3_256::digest(&message.encode_to_vec()[..]);
        Hash256 {
            bytes0_through7   : BigEndian::read_u64(&bytes[0..=7]),
            bytes8_through15  : BigEndian::read_u64(&bytes[8..=15]),
            bytes16_through23 : BigEndian::read_u64(&bytes[16..=23]),
            bytes24_through31 : BigEndian::read_u64(&bytes[24..=31]),
        }
    }

    impl Display for Hash256 {
        /// Display `Hash256` objects using a 64-character Hex representation.
        fn fmt(&self, f : &mut Formatter<'_>) -> Result {
            write!(f, "Hash256{{{:016x}{:016x}{:016x}{:016x}}}",
                self.bytes0_through7,
                self.bytes8_through15,
                self.bytes16_through23,
                self.bytes24_through31)
        }
    }

    /// Ironically, we want to make Hash256 objects hashable
    impl Hash for Hash256 {
        /// Hash a `Hash256` object by hashing all its bytes.
        fn hash<H: Hasher>(&self, state : &mut H) {
            self.bytes0_through7.hash(state);
            self.bytes8_through15.hash(state);
            self.bytes16_through23.hash(state);
            self.bytes24_through31.hash(state);
        }
    }

    impl Eq for Hash256 {}

    /// We want to be able to compare hashes using < etc.
    impl Ord for Hash256 {
        /// `Hash256` ordering is defined by just considering them as a 4-tuple of i64s.
        fn cmp(&self, other: &Self) -> Ordering {
            (self.bytes0_through7,
             self.bytes8_through15,
             self.bytes16_through23,
             self.bytes24_through31).cmp(
           &(other.bytes0_through7,
             other.bytes8_through15,
             other.bytes16_through23,
             other.bytes24_through31))
        }
    }

    /// We want to be able to compare hashes using < etc.
    impl PartialOrd for Hash256 {
        /// `Hash256` ordering is defined by just considering them as a 4-tuple of i64s.
        /// This uses the `Ord` implementation of `Hash256`.
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Eq for Ballot {}

    /// We want to be able to compare ballots using < etc.
    impl Ord for Ballot {
        /// Ordering is by timestamp (0 used if no Timestamp is available), then value hash.
        fn cmp(&self, other: &Self) -> Ordering {
            fn timestamp_tuple(timestamp : &Option<Timestamp>) -> (i64, i32) {
                if let Some(t) = timestamp {
                   (t.seconds, t.nanos)
                } else {
                   (0, 0)
                }
            }
            (timestamp_tuple(&self.timestamp), &self.value_hash).cmp(
             &(timestamp_tuple(&other.timestamp), &other.value_hash))
        }
    }

    /// We want to be able to compare ballots using < etc.
    impl PartialOrd for Ballot {
        /// Ordering is by timestamp (0 used if no Timestamp is available), then value hash.
        /// This uses `Ballot`'s `Ord` implementation.
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }
}


#[cfg(test)]
mod tests {
    /// A bloody worthless test case serving only as an example
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
