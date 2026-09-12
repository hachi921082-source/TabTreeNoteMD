# Third-Party Notices

TabTreeNoteMD is distributed as a closed-source built application, but it
incorporates a number of open-source third-party components. This document
lists every third-party package that ships inside the built app (frontend
npm dependencies and Rust/Cargo dependencies of the `src-tauri` backend),
together with each package's license, to satisfy the notice-preservation
requirements of the MIT, Apache-2.0, ISC, BSD-3-Clause, and CC0-1.0 licenses
used by those packages.

This list reflects the dependency set at the time it was last updated. It is
reviewed before each release build (see the project's `npm run tauri build`
pre-build check) and may lag a small amount behind `package.json` /
`src-tauri/Cargo.toml` between reviews.

No GPL, LGPL, or AGPL licensed component is used anywhere in this app.

## Index

1. [Frontend (npm) Dependencies](#1-frontend-npm-dependencies)
2. [Backend (Rust / Cargo) Dependencies](#2-backend-rust--cargo-dependencies)
3. [License Texts](#3-license-texts)
   - [3.1 MIT License](#31-mit-license)
   - [3.2 Apache License 2.0](#32-apache-license-20)
   - [3.3 ISC License](#33-isc-license)
   - [3.4 BSD 3-Clause License](#34-bsd-3-clause-license)
   - [3.5 CC0 1.0 Universal](#35-cc0-10-universal)

## 1. Frontend (npm) Dependencies

| Package | License |
|---|---|
| @codemirror/commands | MIT |
| @codemirror/lang-markdown | MIT |
| @codemirror/language | MIT |
| @codemirror/search | MIT |
| @codemirror/state | MIT |
| @codemirror/view | MIT |
| @lezer/markdown | MIT |
| @tauri-apps/api | Apache-2.0 OR MIT (used here under MIT) |
| dompurify | MPL-2.0 OR Apache-2.0 — **Apache-2.0 elected** (see note below) |
| jaconv | MIT |
| katex | MIT |
| lucide-react | ISC |
| markdown-it | MIT |
| markdown-it-task-lists | ISC |
| markdown-it-texmath | MIT |
| react | MIT |
| react-dom | MIT |

**Note on dompurify:** dompurify is dual-licensed under MPL-2.0 or Apache-2.0,
at the user's option. TabTreeNoteMD explicitly elects the **Apache-2.0**
license for its use of dompurify, to avoid MPL-2.0's per-file copyleft
obligations in this closed-source distribution. The Apache License 2.0 text
is reproduced in [3.2](#32-apache-license-20).

## 2. Backend (Rust / Cargo) Dependencies

Excludes dev-dependencies, which are not compiled into the shipped
application.

| Crate | License |
|---|---|
| tauri | Apache-2.0 OR MIT (used here under MIT) |
| tauri-build | Apache-2.0 OR MIT (used here under MIT) |
| tauri-plugin-dialog | Apache-2.0 OR MIT (used here under MIT) |
| tauri-plugin-opener | Apache-2.0 OR MIT (used here under MIT) |
| tauri-plugin-single-instance | Apache-2.0 OR MIT (used here under MIT) |
| tauri-plugin-autostart | Apache-2.0 OR MIT (used here under MIT) |
| serde | MIT OR Apache-2.0 (used here under MIT) |
| serde_json | MIT OR Apache-2.0 (used here under MIT) |
| notify | CC0-1.0 |
| notify-debouncer-full | MIT OR Apache-2.0 (used here under MIT) |
| sha2 | MIT OR Apache-2.0 (used here under MIT) |
| trash | MIT |
| tracing | MIT |
| tracing-subscriber | MIT |
| tokio | MIT |
| rand | MIT OR Apache-2.0 (used here under MIT) |
| aes-gcm | Apache-2.0 OR MIT (used here under MIT) |
| scrypt | MIT OR Apache-2.0 (used here under MIT) |
| base64 | MIT OR Apache-2.0 (used here under MIT) |
| encoding_rs | (Apache-2.0 OR MIT) AND BSD-3-Clause (used here under MIT, plus a bundled BSD-3-Clause-licensed component — see note below) |
| regex | MIT OR Apache-2.0 (used here under MIT) |
| image | MIT OR Apache-2.0 (used here under MIT) |

**Note on encoding_rs:** encoding_rs is dual-licensed under Apache-2.0 or
MIT (elected here as MIT for consistency with the rest of the dependency
tree), **and** additionally incorporates data derived from ICU under a
separate BSD-3-Clause license. Both the MIT license ([3.1](#31-mit-license))
and the BSD 3-Clause license ([3.4](#34-bsd-3-clause-license)) apply to this
crate.

## 3. License Texts

### 3.1 MIT License

Used by: @codemirror/commands, @codemirror/lang-markdown,
@codemirror/language, @codemirror/search, @codemirror/state,
@codemirror/view, @lezer/markdown, @tauri-apps/api, jaconv, katex,
markdown-it, markdown-it-texmath, react, react-dom, tauri, tauri-build,
tauri-plugin-dialog, tauri-plugin-opener, tauri-plugin-single-instance,
tauri-plugin-autostart, serde, serde_json, notify-debouncer-full, sha2,
trash, tracing, tracing-subscriber, tokio, rand, aes-gcm, scrypt, base64,
encoding_rs, regex, image.

```
MIT License

Copyright (c) <year> <copyright holders>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to
deal in the Software without restriction, including without limitation the
rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
sell copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
IN THE SOFTWARE.
```

*(Exact copyright holder names/years are as published by each package in
its own repository; this notice reproduces the standard MIT text shared by
all of the packages listed above.)*

### 3.2 Apache License 2.0

Used by (elected/actual license for this build): dompurify (elected, see
[1](#1-frontend-npm-dependencies)).

```
                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the
      purposes of this License, Derivative Works shall not include works
      that remain separable from, or merely link (or bind by name) to the
      interfaces of, the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including the
      original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control
      systems, and issue tracking systems that are managed by, or on behalf
      of, the Licensor for the purpose of discussing and improving the
      Work, but excluding communication that is conspicuously marked or
      otherwise designated in writing by the copyright owner as
      "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and do
          not modify the License. You may add Your own attribution notices
          within Derivative Works that You distribute, alongside or as an
          addendum to the NOTICE text from the Work, provided that such
          additional attribution notices cannot be construed as modifying
          the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing
      the origin of the Work and reproducing the content of the NOTICE
      file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this
      License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS
```

### 3.3 ISC License

Used by: lucide-react, markdown-it-task-lists.

```
ISC License

Copyright (c) <year> <copyright holders>

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR
IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

### 3.4 BSD 3-Clause License

Used by: encoding_rs (bundled ICU-derived component, see
[2](#2-backend-rust--cargo-dependencies)).

```
BSD 3-Clause License

Copyright (c) <year>, <copyright holders>
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice,
   this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.
```

### 3.5 CC0 1.0 Universal

Used by: notify.

notify is dedicated to the public domain under the Creative Commons CC0 1.0
Universal Public Domain Dedication. No rights are reserved, and no
attribution is legally required; it is listed here for completeness only.
The full legal text of the dedication is available at:

https://creativecommons.org/publicdomain/zero/1.0/legalcode

Summary (not a substitute for the full legal text linked above): the person
who associated a work with this deed has dedicated the work to the public
domain by waiving all of his or her rights to the work worldwide under
copyright law, including all related and neighboring rights, to the extent
allowed by law. The work can be copied, modified, distributed, and
performed, even for commercial purposes, all without asking permission.
