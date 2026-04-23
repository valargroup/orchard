window.BENCHMARK_DATA = {
  "lastUpdate": 1776961642179,
  "repoUrl": "https://github.com/valargroup/orchard",
  "entries": {
    "Orchard Benchmarks": [
      {
        "commit": {
          "author": {
            "email": "jack@electriccoin.co",
            "name": "Jack Grigg",
            "username": "str4d"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "17f835d06587f2cd69ef5931bce371d57848e524",
          "message": "Merge pull request #474 from zcash/release-0.12.0\n\norchard 0.12.0",
          "timestamp": "2025-12-05T17:11:44Z",
          "tree_id": "873cade7725160afc8d56a7146cc4033df64d3d2",
          "url": "https://github.com/zcash/orchard/commit/17f835d06587f2cd69ef5931bce371d57848e524"
        },
        "date": 1764955465897,
        "tool": "cargo",
        "benches": [
          {
            "name": "proving/bundle/1",
            "value": 2683994910,
            "range": "± 202586591",
            "unit": "ns/iter"
          },
          {
            "name": "proving/bundle/2",
            "value": 2676573332,
            "range": "± 4570160",
            "unit": "ns/iter"
          },
          {
            "name": "proving/bundle/3",
            "value": 3858343708,
            "range": "± 4769882",
            "unit": "ns/iter"
          },
          {
            "name": "proving/bundle/4",
            "value": 5017598332,
            "range": "± 15060267",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/1",
            "value": 20953219,
            "range": "± 128159",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/2",
            "value": 21085723,
            "range": "± 183708",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/3",
            "value": 24333441,
            "range": "± 213497",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/4",
            "value": 27653348,
            "range": "± 249032",
            "unit": "ns/iter"
          },
          {
            "name": "note-decryption/valid",
            "value": 1471043,
            "range": "± 7367",
            "unit": "ns/iter"
          },
          {
            "name": "note-decryption/invalid",
            "value": 125458,
            "range": "± 177",
            "unit": "ns/iter"
          },
          {
            "name": "note-decryption/compact-valid",
            "value": 1467663,
            "range": "± 4915",
            "unit": "ns/iter"
          },
          {
            "name": "compact-note-decryption/invalid",
            "value": 1334335416,
            "range": "± 1482164",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/valid/10",
            "value": 15536565,
            "range": "± 28427",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/invalid/10",
            "value": 2130053,
            "range": "± 3995",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-valid/10",
            "value": 15502560,
            "range": "± 65412",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-invalid/10",
            "value": 2094656,
            "range": "± 4483",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/valid/50",
            "value": 77625235,
            "range": "± 166587",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/invalid/50",
            "value": 10595083,
            "range": "± 13872",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-valid/50",
            "value": 77458547,
            "range": "± 136442",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-invalid/50",
            "value": 10420664,
            "range": "± 20945",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/valid/100",
            "value": 155265105,
            "range": "± 140015",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/invalid/100",
            "value": 21175560,
            "range": "± 30719",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-valid/100",
            "value": 154908416,
            "range": "± 118853",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-invalid/100",
            "value": 20828646,
            "range": "± 33069",
            "unit": "ns/iter"
          },
          {
            "name": "derive_fvk",
            "value": 461245,
            "range": "± 1241",
            "unit": "ns/iter"
          },
          {
            "name": "default_address",
            "value": 488274,
            "range": "± 794",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "kris@nutty.land",
            "name": "Kris Nuttycombe",
            "username": "nuttycom"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "64599b81ef4fee59c41a5b98619e27a8be38f953",
          "message": "Merge pull request #477 from zcash/pczt_extract_reference\n\nMake pczt::Bundle::extract take `self` by reference.",
          "timestamp": "2026-03-03T09:50:03-08:00",
          "tree_id": "32086be78565bb131d6647582abacf35963dccfe",
          "url": "https://github.com/valargroup/orchard/commit/64599b81ef4fee59c41a5b98619e27a8be38f953"
        },
        "date": 1775868502117,
        "tool": "cargo",
        "benches": [
          {
            "name": "proving/bundle/1",
            "value": 2788941437,
            "range": "± 244926837",
            "unit": "ns/iter"
          },
          {
            "name": "proving/bundle/2",
            "value": 2775134700,
            "range": "± 31695398",
            "unit": "ns/iter"
          },
          {
            "name": "proving/bundle/3",
            "value": 3971156454,
            "range": "± 38217053",
            "unit": "ns/iter"
          },
          {
            "name": "proving/bundle/4",
            "value": 5169036217,
            "range": "± 6193983",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/1",
            "value": 22779629,
            "range": "± 177787",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/2",
            "value": 22656127,
            "range": "± 506016",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/3",
            "value": 26147624,
            "range": "± 411284",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/4",
            "value": 29697140,
            "range": "± 246490",
            "unit": "ns/iter"
          },
          {
            "name": "note-decryption/valid",
            "value": 1614395,
            "range": "± 42982",
            "unit": "ns/iter"
          },
          {
            "name": "note-decryption/invalid",
            "value": 137426,
            "range": "± 10830",
            "unit": "ns/iter"
          },
          {
            "name": "note-decryption/compact-valid",
            "value": 1611703,
            "range": "± 10599",
            "unit": "ns/iter"
          },
          {
            "name": "compact-note-decryption/invalid",
            "value": 1459695275,
            "range": "± 9880933",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/valid/10",
            "value": 17054199,
            "range": "± 78023",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/invalid/10",
            "value": 2320398,
            "range": "± 6263",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-valid/10",
            "value": 17016198,
            "range": "± 24938",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-invalid/10",
            "value": 2280435,
            "range": "± 6101",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/valid/50",
            "value": 85166659,
            "range": "± 2967628",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/invalid/50",
            "value": 11553088,
            "range": "± 234169",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-valid/50",
            "value": 84985984,
            "range": "± 116519",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-invalid/50",
            "value": 11336743,
            "range": "± 29742",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/valid/100",
            "value": 170305405,
            "range": "± 510467",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/invalid/100",
            "value": 23105037,
            "range": "± 1417574",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-valid/100",
            "value": 170092570,
            "range": "± 6102306",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-invalid/100",
            "value": 22657540,
            "range": "± 56476",
            "unit": "ns/iter"
          },
          {
            "name": "derive_fvk",
            "value": 505173,
            "range": "± 7322",
            "unit": "ns/iter"
          },
          {
            "name": "default_address",
            "value": 535988,
            "range": "± 1260",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "kris@nutty.land",
            "name": "Kris Nuttycombe",
            "username": "nuttycom"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "7e90bf2a2c1c6d766496585a045ca110d6b8f1d8",
          "message": "Merge pull request #480 from ebfull/orchard_internal\n\nSplit orchard into `orchard_internal` (wide visibility) + `orchard` (API-preserving shim)",
          "timestamp": "2026-04-20T09:02:24-06:00",
          "tree_id": "5cdc4c5902c76162c38005200716dc4590e34435",
          "url": "https://github.com/valargroup/orchard/commit/7e90bf2a2c1c6d766496585a045ca110d6b8f1d8"
        },
        "date": 1776712804105,
        "tool": "cargo",
        "benches": [
          {
            "name": "proving/bundle/1",
            "value": 2722576467,
            "range": "± 27036160",
            "unit": "ns/iter"
          },
          {
            "name": "proving/bundle/2",
            "value": 2719779327,
            "range": "± 6009761",
            "unit": "ns/iter"
          },
          {
            "name": "proving/bundle/3",
            "value": 3893063677,
            "range": "± 32270160",
            "unit": "ns/iter"
          },
          {
            "name": "proving/bundle/4",
            "value": 5069858033,
            "range": "± 4662416",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/1",
            "value": 22106175,
            "range": "± 151597",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/2",
            "value": 22209460,
            "range": "± 994067",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/3",
            "value": 25428739,
            "range": "± 197630",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/4",
            "value": 28991522,
            "range": "± 260212",
            "unit": "ns/iter"
          },
          {
            "name": "note-decryption/valid",
            "value": 1580726,
            "range": "± 9541",
            "unit": "ns/iter"
          },
          {
            "name": "note-decryption/invalid",
            "value": 134267,
            "range": "± 247",
            "unit": "ns/iter"
          },
          {
            "name": "note-decryption/compact-valid",
            "value": 1578486,
            "range": "± 3151",
            "unit": "ns/iter"
          },
          {
            "name": "compact-note-decryption/invalid",
            "value": 1409096105,
            "range": "± 3591712",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/valid/10",
            "value": 16714675,
            "range": "± 22043",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/invalid/10",
            "value": 2285150,
            "range": "± 5201",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-valid/10",
            "value": 16676020,
            "range": "± 44255",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-invalid/10",
            "value": 2243529,
            "range": "± 3510",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/valid/50",
            "value": 83506372,
            "range": "± 112088",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/invalid/50",
            "value": 11364194,
            "range": "± 15590",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-valid/50",
            "value": 83338798,
            "range": "± 388032",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-invalid/50",
            "value": 11158830,
            "range": "± 21042",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/valid/100",
            "value": 167007643,
            "range": "± 1937611",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/invalid/100",
            "value": 22720406,
            "range": "± 34239",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-valid/100",
            "value": 166627502,
            "range": "± 371139",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-invalid/100",
            "value": 22301178,
            "range": "± 30057",
            "unit": "ns/iter"
          },
          {
            "name": "derive_fvk",
            "value": 488077,
            "range": "± 12639",
            "unit": "ns/iter"
          },
          {
            "name": "default_address",
            "value": 521989,
            "range": "± 663",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "kris@nutty.land",
            "name": "Kris Nuttycombe",
            "username": "nuttycom"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "081c2ac4fa0395f808a1d1e9b6d0c8e2744a53b5",
          "message": "Merge pull request #493 from ebfull/undo-orchard-internal\n\nRevert `orchard_internal` crate split",
          "timestamp": "2026-04-22T15:01:11-06:00",
          "tree_id": "404bdeafe2501ae80bfb427e0cfb3f5a9396a06e",
          "url": "https://github.com/valargroup/orchard/commit/081c2ac4fa0395f808a1d1e9b6d0c8e2744a53b5"
        },
        "date": 1776893600991,
        "tool": "cargo",
        "benches": [
          {
            "name": "proving/bundle/1",
            "value": 2596667041,
            "range": "± 16967730",
            "unit": "ns/iter"
          },
          {
            "name": "proving/bundle/2",
            "value": 2599980118,
            "range": "± 15333427",
            "unit": "ns/iter"
          },
          {
            "name": "proving/bundle/3",
            "value": 3694824895,
            "range": "± 45790060",
            "unit": "ns/iter"
          },
          {
            "name": "proving/bundle/4",
            "value": 4794494923,
            "range": "± 17930124",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/1",
            "value": 20589797,
            "range": "± 252809",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/2",
            "value": 20599376,
            "range": "± 146253",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/3",
            "value": 23725046,
            "range": "± 899939",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/4",
            "value": 26999499,
            "range": "± 974784",
            "unit": "ns/iter"
          },
          {
            "name": "note-decryption/valid",
            "value": 1474236,
            "range": "± 9645",
            "unit": "ns/iter"
          },
          {
            "name": "note-decryption/invalid",
            "value": 124719,
            "range": "± 147",
            "unit": "ns/iter"
          },
          {
            "name": "note-decryption/compact-valid",
            "value": 1470570,
            "range": "± 3953",
            "unit": "ns/iter"
          },
          {
            "name": "compact-note-decryption/invalid",
            "value": 1323893497,
            "range": "± 3372967",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/valid/10",
            "value": 15557083,
            "range": "± 20314",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/invalid/10",
            "value": 2111349,
            "range": "± 34488",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-valid/10",
            "value": 15514078,
            "range": "± 187311",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-invalid/10",
            "value": 2076325,
            "range": "± 3233",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/valid/50",
            "value": 77732187,
            "range": "± 102206",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/invalid/50",
            "value": 10498873,
            "range": "± 105257",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-valid/50",
            "value": 77541514,
            "range": "± 117015",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-invalid/50",
            "value": 10325888,
            "range": "± 15304",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/valid/100",
            "value": 155719287,
            "range": "± 367024",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/invalid/100",
            "value": 21051037,
            "range": "± 248958",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-valid/100",
            "value": 155181360,
            "range": "± 203406",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-invalid/100",
            "value": 20673583,
            "range": "± 31295",
            "unit": "ns/iter"
          },
          {
            "name": "derive_fvk",
            "value": 453608,
            "range": "± 2277",
            "unit": "ns/iter"
          },
          {
            "name": "default_address",
            "value": 488130,
            "range": "± 3741",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "kris@nutty.land",
            "name": "Kris Nuttycombe",
            "username": "nuttycom"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "7a1277e4be57b8944395b66da538ae37cadfc500",
          "message": "Merge pull request #494 from zcash/release-0.13.0\n\nRelease orchard v0.13.0",
          "timestamp": "2026-04-22T17:53:58-06:00",
          "tree_id": "7028a8ba87339eac34e87436fc8963f17954dbb5",
          "url": "https://github.com/valargroup/orchard/commit/7a1277e4be57b8944395b66da538ae37cadfc500"
        },
        "date": 1776961641393,
        "tool": "cargo",
        "benches": [
          {
            "name": "proving/bundle/1",
            "value": 2589200386,
            "range": "± 142071022",
            "unit": "ns/iter"
          },
          {
            "name": "proving/bundle/2",
            "value": 2561732286,
            "range": "± 3827544",
            "unit": "ns/iter"
          },
          {
            "name": "proving/bundle/3",
            "value": 3661702628,
            "range": "± 18494565",
            "unit": "ns/iter"
          },
          {
            "name": "proving/bundle/4",
            "value": 4823114745,
            "range": "± 43829179",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/1",
            "value": 20612912,
            "range": "± 157542",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/2",
            "value": 20415638,
            "range": "± 128040",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/3",
            "value": 23706397,
            "range": "± 188385",
            "unit": "ns/iter"
          },
          {
            "name": "verifying/bundle/4",
            "value": 26669953,
            "range": "± 247064",
            "unit": "ns/iter"
          },
          {
            "name": "note-decryption/valid",
            "value": 1485404,
            "range": "± 7628",
            "unit": "ns/iter"
          },
          {
            "name": "note-decryption/invalid",
            "value": 124951,
            "range": "± 2426",
            "unit": "ns/iter"
          },
          {
            "name": "note-decryption/compact-valid",
            "value": 1481815,
            "range": "± 12960",
            "unit": "ns/iter"
          },
          {
            "name": "compact-note-decryption/invalid",
            "value": 1317437062,
            "range": "± 4633785",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/valid/10",
            "value": 15669660,
            "range": "± 331221",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/invalid/10",
            "value": 2112913,
            "range": "± 3164",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-valid/10",
            "value": 15634941,
            "range": "± 49934",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-invalid/10",
            "value": 2079002,
            "range": "± 3270",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/valid/50",
            "value": 78291052,
            "range": "± 123620",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/invalid/50",
            "value": 10510710,
            "range": "± 78955",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-valid/50",
            "value": 78096653,
            "range": "± 200516",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-invalid/50",
            "value": 10339646,
            "range": "± 387441",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/valid/100",
            "value": 156513039,
            "range": "± 318288",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/invalid/100",
            "value": 21004210,
            "range": "± 36172",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-valid/100",
            "value": 156157255,
            "range": "± 272685",
            "unit": "ns/iter"
          },
          {
            "name": "batch-note-decryption/compact-invalid/100",
            "value": 20657501,
            "range": "± 283078",
            "unit": "ns/iter"
          },
          {
            "name": "derive_fvk",
            "value": 453735,
            "range": "± 1872",
            "unit": "ns/iter"
          },
          {
            "name": "default_address",
            "value": 488991,
            "range": "± 688",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}