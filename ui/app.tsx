import {Button, Container, Entry, PageContext, Row} from "deft-react";
import {useContext, useEffect, useRef, useState} from "react";
import {useInput} from "./hooks";

const KEY_LAST_SERVER_URI = "last-server-uri";

export function App() {
    const wrapperRef = useRef<ContainerElement>();
    const [uriProps, setUri] = useInput('');
    const [connected, setConnected] = useState(false);
    const pageContext = useContext(PageContext);

    useEffect(() => {
        const lastUri = localStorage.getItem(KEY_LAST_SERVER_URI);
        setUri(lastUri);
    }, []);

    function onConnect() {
        let uri = uriProps.text;
        localStorage.setItem(KEY_LAST_SERVER_URI, uri);
        if (!uri.includes("://")) {
            uri = "spice://" + uri;
        }
        try {
            //@ts-ignore
            const spiceElement = new SpiceElement();
            spiceElement.style = {
                width: '100%',
                height: '100%',
            }
            spiceElement.connect(uri);
            spiceElement.bindConnectSuccess(() => {
                console.log("connect success");
                wrapperRef.current.addChild(spiceElement);
                setConnected(true);
            })
            spiceElement.bindConnectFail((e) => {
                console.log("connect error", e);
                pageContext.window.toast(e.detail.message);
            })
            spiceElement.bindDisplayClose(() => {
                console.log("Disconnected");
                process.exit(0);
            });
        } catch (error) {
            console.error(error);
        }
    }

    const connectPanel = connected ? null : <Row style={{
        position: 'absolute',
        width: '100%',
        height: '100%',
        justifyContent: 'center',
        alignItems: 'center',
        maxWidth: 400,
    }}>
        <Row>Server URI: </Row>
        <Entry {...uriProps} style={{flex: 1}}/>
        <Button onClick={onConnect}>Connect</Button>
    </Row>

    return <Container style={{
        background: "#2a2a2a",
        color: "#FFF",
        justifyContent: 'center',
        alignItems: 'center',
        height: '100%',
        position: 'relative',
    }}>
        <Container ref={wrapperRef} style={{
            width: '100%',
            height: '100%',
        }}></Container>
        {connectPanel}
    </Container>
}
