//! Replays of the engine without the certificate (dev 8ca8d40d) for
//! `super::pins`. A failing pin prints its replacement; regenerate only for an
//! intended behaviour change, on an engine that has no mover certificate.
//! Every scene has refused proposals: steps a wrong certificate would take.
use super::Pin;

pub(super) const HEAD_ON: Pin = Pin {
    digest: "0297cd5472a735fb05d98d5539a00d22b7e73982604024cd07629023194959fb",
    marks: "badcb5f5 53affcf5 87e1088d 963dcd82 04fb4c89 3b188f36 ccae50d6 14d35f68",
    refused: 184,
    overlaps: 0,
};

pub(super) const OVERTAKING: Pin = Pin {
    digest: "82c47c15dd13fb8b28619b00524f1e4791e69856bc99d976de676c6e0bae8206",
    marks: "e6b0813d 07ae23d4 60692b1a eb59d658 d6db0fad 5356675f d00d26fd fba39428",
    refused: 90,
    overlaps: 0,
};

pub(super) const DASHING: Pin = Pin {
    digest: "4ac98cffcaa3e92aa3b04630eadae06e49a1c242cabee10577ff18cff41b191d",
    marks: "78d220b6 879ba85d eae20cb4 3ee26f2e 5a448656 3b3d4703",
    refused: 8,
    overlaps: 0,
};

pub(super) const ROAD: Pin = Pin {
    digest: "3ed0b4802e85d4bbd53d97dc75d29ee97e9156078e3bd31037aa6f8ee11b1d4f",
    marks: "7bec10a8 4915b33e 407114e1 0e5fb107 b9ef1665 b4ef8e17 c0ae3988",
    refused: 44,
    overlaps: 0,
};

pub(super) const PICKETS: Pin = Pin {
    digest: "48c21a0f4fa1e5ce6643401bcfb0e7c42ad6bf1324c09dd2ae623b64d4f680ee",
    marks: "3b2bf9a4 794a309a 5aa0f757 48e4c8a1 d986ed34 c4748776 1eea272b 3c0760e3",
    refused: 26,
    overlaps: 0,
};

pub(super) const ARRIVALS: Pin = Pin {
    digest: "3476d2de8f029412e1a2ac324700852166b62bc20acc5bd881bb14d2e7293af0",
    marks: "aec0a1d2 29f27c19 338cfe3a 953e7135 279004e9 b4d20430 86864af9 b36c5152",
    refused: 10,
    overlaps: 0,
};

pub(super) const OVERFULL: Pin = Pin {
    digest: "7973c39b007af93ca890a2a6bb1cc82a7560fb6fe7c41e1b47c35b273f72210a",
    marks: "145d7f10 6c54023a 887cc3d8 4143dafc 351e6b3b f0aa4144",
    refused: 29,
    overlaps: 0,
};
