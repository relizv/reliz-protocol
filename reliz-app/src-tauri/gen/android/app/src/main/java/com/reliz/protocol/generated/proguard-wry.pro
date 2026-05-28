# THIS FILE IS AUTO-GENERATED. DO NOT MODIFY!!

# Copyright 2020-2023 Tauri Programme within The Commons Conservancy
# SPDX-License-Identifier: Apache-2.0
# SPDX-License-Identifier: MIT

-keep class com.reliz.protocol.* {
  native <methods>;
}

-keep class com.reliz.protocol.WryActivity {
  public <init>(...);

  void setWebView(com.reliz.protocol.RustWebView);
  java.lang.Class getAppClass(...);
  int getId();
  java.lang.String getVersion();
  int startActivity(...);
}

-keep class com.reliz.protocol.Ipc {
  public <init>(...);

  @android.webkit.JavascriptInterface public <methods>;
}

-keep class com.reliz.protocol.RustWebView {
  public <init>(...);

  void loadUrlMainThread(...);
  void loadHTMLMainThread(...);
  void evalScript(...);
}

-keep class com.reliz.protocol.RustWebChromeClient,com.reliz.protocol.RustWebViewClient {
  public <init>(...);
}
