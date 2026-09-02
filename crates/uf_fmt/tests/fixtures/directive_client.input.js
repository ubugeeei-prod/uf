'use client'
// @flow
import {useState} from 'react'

component Counter() renders React.Node {
const [count,setCount]=useState(0)
return <button onClick={()=>setCount(count+1)}>{count}</button>
}

export default Counter
