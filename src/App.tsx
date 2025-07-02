import { useEffect, useState, useRef } from "react"
import { Loader2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  InputOTP,
  InputOTPGroup,
  InputOTPSlot,
} from "@/components/ui/input-otp"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import logo from "@/assets/logo.svg"
import { REGEXP_ONLY_DIGITS } from "input-otp"
import { ReactSVG } from "react-svg"

declare global {
  interface Window {
    peerConnection: RTCPeerConnection | null;
  }
}

export default function ConnectionPage() {
  const [session, setSession] = useState("");
  const [otpValue, setOtpValue] = useState("");
  const [deviceName, setDeviceName] = useState("");
  const [disabled, setDisabled] = useState(true);
  const [idDisabled, setIdDisabled] = useState(true);
  const [loading, setLoading] = useState(false);
  const [errorMessage, setErrorMessage] = useState("");

  const videoRef = useRef(null as HTMLVideoElement | null);
  const containerRef = useRef(null as HTMLDivElement | null);
  const [isConnected, setIsConnected] = useState(false);

  const generateCrossBrowserStyle = (attribute: string, value: string | number): Record<string, string | number> => {
    const webkit = `Webkit${attribute.charAt(0).toUpperCase() + attribute.slice(1)}`;
    const ms = `ms${attribute.charAt(0).toUpperCase() + attribute.slice(1)}`;
    const o = `O${attribute.charAt(0).toUpperCase() + attribute.slice(1)}`;
    const moz = `Moz${attribute.charAt(0).toUpperCase() + attribute.slice(1)}`;
    return {
      [attribute]: value,
      [webkit]: value,
      [ms]: value,
      [o]: value,
      [moz]: value,
      [attribute]: value,
    };
  };

  const combineStyles = (styles1: Record<string, string | number>, styles2: Record<string, string | number>): Record<string, string | number> => {
    const styles = {} as Record<string, string | number>;
    for (const style1 in styles1) {
      styles[style1] = styles1[style1];
    }
    for (const style2 in styles2) {
      styles[style2] = styles2[style2];
    }
    return styles;
  };

  useEffect(() => {
    const id = new URL(window.location.href).pathname.match(/[^/]+/g);
    if (id && id.length > 0 && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-5][0-9a-f]{3}-[089ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test([...id][id.length-1])) {
      setSession([...id][id.length-1]);
    } else {
      setIdDisabled(false);
    }
    const peerConnection = new RTCPeerConnection({
      iceServers: [{ urls: "stun:stun.l.google.com:19302" }]
    });
    window.peerConnection = peerConnection;
    const element = containerRef.current!;
    const handleKeyPress = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        if (document.exitFullscreen) {
          document.exitFullscreen();
          // @ts-expect-error vendor specific methods
        } else if (document.webkitExitFullscreen) {
          // @ts-expect-error vendor specific methods
          document.webkitExitFullscreen();
          // @ts-expect-error vendor specific methods
        } else if (document.mozCancelFullScreen) {
          // @ts-expect-error vendor specific methods
          document.mozCancelFullScreen();
          // @ts-expect-error vendor specific methods
        } else if (document.msExitFullscreen) {
          // @ts-expect-error vendor specific methods
          document.msExitFullscreen();
        }
        if (document.exitPointerLock) {
          document.exitPointerLock();
          // @ts-expect-error vendor specific methods
        } else if (document.mozExitPointerLock) {
          // @ts-expect-error vendor specific methods
          document.mozExitPointerLock();
          // @ts-expect-error vendor specific methods
        } else if (document.webkitExitPointerLock) {
          // @ts-expect-error vendor specific methods
          document.webkitExitPointerLock();
        } else {
          element.style.cursor = "auto";
          element.style.userSelect = "auto";
          element.style.webkitUserSelect = "auto";
          // @ts-expect-error vendor specific prefix
          element.style.mozUserSelect = "auto";
          // @ts-expect-error vendor specific prefix
          element.style.msUserSelect = "auto";
          // @ts-expect-error vendor specific prefix
          element.style.osUserSelect = "auto";
        }
        setIsConnected(false);
        window.peerConnection!.close();
        const peerConnection = new RTCPeerConnection({
          iceServers: [{ urls: "stun:stun.l.google.com:19302" }]
        });
        window.peerConnection = peerConnection;
      }
    };
    const exitHandler = () => {
      // @ts-expect-error vendor specific methods
      const fullscreenElement = document.fullscreenElement || document.mozFullScreenElement || document.webkitFullscreenElement;
      if (fullscreenElement == null) {
        if (document.exitPointerLock) {
          document.exitPointerLock();
          // @ts-expect-error vendor specific methods
        } else if (document.mozExitPointerLock) {
          // @ts-expect-error vendor specific methods
          document.mozExitPointerLock();
          // @ts-expect-error vendor specific methods
        } else if (document.webkitExitPointerLock) {
          // @ts-expect-error vendor specific methods
          document.webkitExitPointerLock();
        } else {
          element.style.cursor = "auto";
          element.style.userSelect = "auto";
          element.style.webkitUserSelect = "auto";
          // @ts-expect-error vendor specific prefix
          element.style.mozUserSelect = "auto";
          // @ts-expect-error vendor specific prefix
          element.style.msUserSelect = "auto";
          // @ts-expect-error vendor specific prefix
          element.style.osUserSelect = "auto";
        }
        setIsConnected(false);
        window.peerConnection!.close();
        const peerConnection = new RTCPeerConnection({
          iceServers: [{ urls: "stun:stun.l.google.com:19302" }]
        });
        window.peerConnection = peerConnection;
      }
    };
    element.addEventListener("fullscreenchange", exitHandler);
    element.addEventListener("mozfullscreenchange", exitHandler);
    element.addEventListener("MSFullscreenChange", exitHandler);
    element.addEventListener("webkitfullscreenchange", exitHandler);
    element.addEventListener("keydown", handleKeyPress);
    return () => element.removeEventListener("keydown", handleKeyPress);
  }, []);

  useEffect(() => {
    if (isConnected) {
      const lockDevice = async () => {
        const element = containerRef.current!;
        element.tabIndex = -1;
        element.focus();
        if (element.requestFullscreen) {
          await element.requestFullscreen();
          // @ts-expect-error vendor specific methods
        } else if (element.webkitRequestFullscreen) {
          // @ts-expect-error vendor specific methods
          await element.webkitRequestFullscreen();
          // @ts-expect-error vendor specific methods
        } else if (element.mozRequestFullScreen) {
          // @ts-expect-error vendor specific methods
          await element.mozRequestFullScreen();
          // @ts-expect-error vendor specific methods
        } else if (element.msRequestFullscreen) {
          // @ts-expect-error vendor specific methods
          await element.msRequestFullscreen();
          // @ts-expect-error vendor specific methods
        } else if (typeof window.ActiveXObject !== "undefined") {
          // @ts-expect-error vendor specific methods
          const wscript = new ActiveXObject("WScript.Shell");
          if (wscript !== null) {
            wscript.SendKeys("{F11}");
          }
        }
        setTimeout(() => {
          if (element.requestPointerLock) {
            element.requestPointerLock();
            // @ts-expect-error vendor specific methods
          } else if (element.mozRequestPointerLock) {
            // @ts-expect-error vendor specific methods
            element.mozRequestPointerLock();
            // @ts-expect-error vendor specific methods
          } else if (element.webkitRequestPointerLock) {
            // @ts-expect-error vendor specific methods
            element.webkitRequestPointerLock();
          } else {
            element.style.cursor = "none";
            element.style.userSelect = "none";
            element.style.webkitUserSelect = "none";
            // @ts-expect-error vendor specific prefix
            element.style.mozUserSelect = "none";
            // @ts-expect-error vendor specific prefix
            element.style.msUserSelect = "none";
            // @ts-expect-error vendor specific prefix
            element.style.osUserSelect = "none";
          }
        }, 100);
      };
      lockDevice();
    }
  }, [isConnected]);

  useEffect(() => {
    if (otpValue.length === 6 && deviceName.length > 0 && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-5][0-9a-f]{3}-[089ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(session)) {
      setDisabled(false);
    } else {
      setDisabled(true);
    }
  }, [session, otpValue, deviceName]);

  return (
    <div className="bg-muted flex min-h-svh flex-col items-center justify-center gap-6 p-6 md:p-10">
      <div className="flex w-full max-w-sm flex-col gap-6" style={{ zIndex: isConnected ? 0 : 99 }}>
        <Card className="font-medium flex-row items-center justify-center w-fit self-center py-2 px-3 gap-2 cursor-pointer" onClick={() => {
          window.open("https://screenextend.app/", "_blank")
        }}>
          <ReactSVG src={logo} className="size-4" />
          <span>ScreenExtend</span>
        </Card>
        <div className="flex flex-col gap-6">
          <Card>
            <CardHeader className="text-center">
              <CardTitle className="text-xl">Join a Session</CardTitle>
              <CardDescription>
                Enter the OTP from the settings screen
              </CardDescription>
            </CardHeader>
            <CardContent>
              <form autoComplete="off" noValidate>
                <Input autoComplete="false" name="hidden" type="text" style={{ display: "none", visibility: "hidden", opacity: 0 }}></Input>
                <div style={loading ? combineStyles({ position: "absolute", left: "50%", top: "50%", zIndex: 999 }, generateCrossBrowserStyle("transform", "translate(-50%, -50%)")) : {display: "none"}} className={loading ? "flex flex-col items-center" : ""}>
                  <Loader2 className="animate-spin mt-5" size={48} />
                  <p className="text-xl font-semibold">Connecting</p>
                </div>
                <div className="grid gap-6" style={loading ? combineStyles(combineStyles({ opacity: 0.75, userSelect: "none", pointerEvents: "none" }, generateCrossBrowserStyle("filter", "blur(5px)")), generateCrossBrowserStyle("transition", "filter 0.5s ease-in-out, opacity 0.5s")) : {}}>
                  <div className="grid gap-6">
                    <div className="grid gap-3">
                      <Label htmlFor="session">Session ID<span className="text-red-500" style={{ marginLeft: -5 }}>*</span></Label>
                      <Input
                        id="session"
                        type="text"
                        placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
                        value={session}
                        onChange={(event) => setSession(event.target.value.slice(0, 20))}
                        disabled={idDisabled}
                      />
                    </div>
                    <div className="grid gap-3">
                      <Label htmlFor="name">Device Name<span className="text-red-500" style={{ marginLeft: -5 }}>*</span></Label>
                      <Input
                        id="name"
                        type="text"
                        placeholder="Laptop"
                        value={deviceName}
                        onChange={(event) => setDeviceName(event.target.value.slice(0, 20))}
                      />
                    </div>
                    <div className="grid gap-3">
                      <div className="flex items-center">
                        <Label htmlFor="otp">OTP<span className="text-red-500" style={{ marginLeft: -5 }}>*</span></Label>
                      </div>
                      <InputOTP maxLength={6} pattern={REGEXP_ONLY_DIGITS} value={otpValue} onChange={(value) => setOtpValue(value)}>
                        <InputOTPGroup style={{ display: "flex", width: "100%" }}>
                          <InputOTPSlot index={0} style={{ flex: 1 }} />
                          <InputOTPSlot index={1} style={{ flex: 1 }} />
                          <InputOTPSlot index={2} style={{ flex: 1 }} />
                          <InputOTPSlot index={3} style={{ flex: 1 }} />
                          <InputOTPSlot index={4} style={{ flex: 1 }} />
                          <InputOTPSlot index={5} style={{ flex: 1 }} />
                        </InputOTPGroup>
                      </InputOTP>
                    </div>
                    <div className={disabled ? "opacity-50 cursor-not-allowed select-none" : ""}>
                      <Button type="submit" className="w-full" disabled={disabled} onClick={async (event) => {
                        event.preventDefault();
                        setErrorMessage("");
                        setLoading(true);
                        window.peerConnection!.ontrack = (event) => {
                          if (videoRef.current && event.streams[0]) {
                            videoRef.current.srcObject = event.streams[0];
                            setIsConnected(true);
                          }
                        };
                        window.peerConnection!.addTransceiver("video", {"direction": "recvonly"});
                        const offer = await window.peerConnection!.createOffer();
                        await window.peerConnection!.setLocalDescription(offer);
                        const request = await fetch("https://backend.screenextend.app/register", {
                          method: "POST",
                          headers: {
                            "Accept": "application/json",
                            "Content-Type": "application/json"
                          },
                          body: JSON.stringify({
                            id: session,
                            otp: deviceName,
                            name: otpValue,
                            sdp: offer.sdp
                          })
                        });
                        const response = await request.json();
                        await new Promise(resolve => setTimeout(resolve, 3000));
                        if (response.success) {
                          await window.peerConnection!.setRemoteDescription({
                            type: "answer",
                            sdp: response.sdp
                          });
                        } else {
                          setErrorMessage(response.error);
                        }
                        setLoading(false);
                      }}>
                        Connect
                      </Button>
                    </div>
                  </div>
                </div>
              </form>
              {
                errorMessage && (
                  <div className="text-center text-xs pt-3 text-red-600">
                    Error: {errorMessage}
                  </div>
                )
              }
            </CardContent>
          </Card>
          <div className="text-muted-foreground *:[a]:hover:text-primary text-center text-xs text-balance *:[a]:underline *:[a]:underline-offset-4">
            For persistent errors, please <a href="mailto:support@screenextend.app" className="underline underline-offset-4">contact us</a> with your device information.
          </div>
        </div>
      </div>
      <div ref={containerRef} style={{ position: "absolute", zIndex: isConnected ? 99 : 0, display: isConnected ? "block" : "none" }}>
        <video
          ref={videoRef}
          className="w-full h-full object-cover"
          autoPlay
          muted
          playsInline
          onCanPlay={() => {
            setIsConnected(true);
          }}
          onLoadedData={() => {
            videoRef.current!.play().catch(console.error);
          }}
          onError={() => {
            setErrorMessage("Unable to communicate with the host device. Try restarting ScreenExtend.");
          }}
          style={{
            width: "100vw",
            height: "100vh",
            objectFit: "cover"
          }}
        />
      </div>
    </div>
  )
}
